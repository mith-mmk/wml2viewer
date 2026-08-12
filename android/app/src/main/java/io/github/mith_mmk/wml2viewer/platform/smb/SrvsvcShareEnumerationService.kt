package io.github.mith_mmk.wml2viewer.platform.smb

import com.hierynomus.msdtyp.AccessMask
import com.hierynomus.mserref.NtStatus
import com.hierynomus.msfscc.FileAttributes
import com.hierynomus.mssmb2.SMB2CreateDisposition
import com.hierynomus.mssmb2.SMB2ImpersonationLevel
import com.hierynomus.mssmb2.SMB2ShareAccess
import com.hierynomus.mssmb2.SMBApiException
import com.hierynomus.smbj.SMBClient
import com.hierynomus.smbj.connection.Connection
import com.hierynomus.smbj.session.Session
import com.hierynomus.smbj.share.NamedPipe
import com.hierynomus.smbj.share.PipeShare
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.platform.security.CredentialInvalidatedException
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore
import io.github.mith_mmk.wml2viewer.platform.security.SecretRedactor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.io.EOFException
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.EnumSet
import java.util.concurrent.TimeUnit

internal interface SrvsvcRpcTransport : Closeable {
    fun bind()
    fun call(opnum: Int, requestStub: ByteArray): ByteArray
}

internal fun interface SrvsvcRpcTransportFactory {
    fun open(profile: SmbProfile): SrvsvcRpcTransport
}

internal class SrvsvcAccessDeniedException(cause: Throwable? = null) : IOException("srvsvc access denied", cause)
internal class SrvsvcUnsupportedException(cause: Throwable? = null) : IOException("srvsvc is unsupported", cause)
internal class SrvsvcProtocolException(message: String, cause: Throwable? = null) : IOException(message, cause)

/**
 * Enumerates disk shares through IPC$/srvsvc NetrShareEnum (opnum 15).
 * Manual entry is used only when the server denies RPC enumeration or does not expose srvsvc.
 */
class SrvsvcShareEnumerationService internal constructor(
    private val transportFactory: SrvsvcRpcTransportFactory,
) : SmbShareEnumerationService {
    constructor(credentialStore: CredentialStore) : this(SmbjSrvsvcRpcTransportFactory(credentialStore))

    override suspend fun enumerate(profile: SmbProfile): ShareDiscoveryResult = withContext(Dispatchers.IO) {
        var lastError: Throwable? = null
        repeat(MAX_ATTEMPTS) { attempt ->
            try {
                return@withContext enumerateOnce(profile)
            } catch (error: SrvsvcAccessDeniedException) {
                return@withContext ShareDiscoveryResult.ManualShareRequired("SRVSVC_ACCESS_DENIED")
            } catch (error: SrvsvcUnsupportedException) {
                return@withContext ShareDiscoveryResult.ManualShareRequired("SRVSVC_UNSUPPORTED")
            } catch (error: CredentialInvalidatedException) {
                throw error
            } catch (error: SourceException) {
                if (!error.retryable || attempt == MAX_ATTEMPTS - 1) throw error
                lastError = error
            } catch (error: IOException) {
                lastError = error
                if (attempt == MAX_ATTEMPTS - 1) {
                    throw SourceException(
                        SourceErrorCode.NETWORK,
                        SecretRedactor.redact(error.message).ifBlank { "SMB share enumeration failed" },
                        error,
                        retryable = true,
                    )
                }
            }
            delay(RETRY_DELAYS_MS[attempt])
        }
        throw SourceException(
            SourceErrorCode.NETWORK,
            SecretRedactor.redact(lastError?.message).ifBlank { "SMB share enumeration failed" },
            lastError,
            retryable = true,
        )
    }

    private fun enumerateOnce(profile: SmbProfile): ShareDiscoveryResult {
        transportFactory.open(profile).use { transport ->
            transport.bind()
            val names = linkedSetOf<String>()
            var resumeHandle: Int? = null
            repeat(MAX_PAGES) {
                val response = SrvsvcNdrCodec.decodeShareEnumResponse(
                    transport.call(NETR_SHARE_ENUM_OPNUM, SrvsvcNdrCodec.encodeShareEnumRequest(profile.server, resumeHandle)),
                )
                response.shares.asSequence()
                    .filter { it.type and SHARE_TYPE_MASK == SHARE_TYPE_DISK }
                    .map { SmbPathNormalizer.normalizeShare(it.name) }
                    .forEach(names::add)
                when (response.status) {
                    ERROR_SUCCESS -> return ShareDiscoveryResult.Shares(names.toList())
                    ERROR_MORE_DATA -> resumeHandle = response.resumeHandle
                        ?: throw SrvsvcProtocolException("srvsvc returned more data without a resume handle")
                    ERROR_ACCESS_DENIED -> throw SrvsvcAccessDeniedException()
                    ERROR_NOT_SUPPORTED, ERROR_INVALID_LEVEL, RPC_S_PROCNUM_OUT_OF_RANGE ->
                        throw SrvsvcUnsupportedException()
                    else -> throw SrvsvcProtocolException("NetrShareEnum failed with status ${response.status}")
                }
            }
            throw SrvsvcProtocolException("srvsvc pagination exceeded the safety limit")
        }
    }

    private companion object {
        const val NETR_SHARE_ENUM_OPNUM = 15
        const val MAX_ATTEMPTS = 3
        const val MAX_PAGES = 128
        val RETRY_DELAYS_MS = longArrayOf(250, 750, 1_500)
        const val SHARE_TYPE_MASK = 0xFFFF
        const val SHARE_TYPE_DISK = 0
        const val ERROR_SUCCESS = 0
        const val ERROR_ACCESS_DENIED = 5
        const val ERROR_NOT_SUPPORTED = 50
        const val ERROR_INVALID_LEVEL = 124
        const val ERROR_MORE_DATA = 234
        const val RPC_S_PROCNUM_OUT_OF_RANGE = 1745
    }
}

internal class SmbjSrvsvcRpcTransportFactory(
    private val credentialStore: CredentialStore,
) : SrvsvcRpcTransportFactory {
    override fun open(profile: SmbProfile): SrvsvcRpcTransport = try {
        SmbjSrvsvcRpcTransport(profile, credentialStore)
    } catch (error: SMBApiException) {
        throw mapSrvsvcSmbError(error)
    }
}

private class SmbjSrvsvcRpcTransport(
    profile: SmbProfile,
    credentialStore: CredentialStore,
) : SrvsvcRpcTransport {
    private val client: SMBClient
    private val connection: Connection
    private val session: Session
    private val pipeShare: PipeShare
    private val pipe: NamedPipe
    private var callId = 1
    private var bound = false

    init {
        val createdClient = SMBClient(SmbConnectionSupport.config(profile))
        var createdConnection: Connection? = null
        var createdSession: Session? = null
        var createdPipeShare: PipeShare? = null
        var createdPipe: NamedPipe? = null
        try {
            createdConnection = createdClient.connect(profile.server, profile.port)
            val connectedSession = SmbConnectionSupport.authenticate(createdConnection, profile, credentialStore)
            createdSession = connectedSession
            val status = SmbConnectionSupport.securityStatus(createdConnection, connectedSession)
            if (profile.requireEncryption && !status.encryptionActive) {
                throw SourceException(SourceErrorCode.ACCESS_DENIED, "SMB encryption is required")
            }
            createdPipeShare = connectedSession.connectShare(IPC_SHARE) as? PipeShare
                ?: throw SrvsvcUnsupportedException()
            if (!createdPipeShare.waitForPipe(PIPE_NAME, PIPE_WAIT_SECONDS, TimeUnit.SECONDS)) {
                throw SrvsvcUnsupportedException()
            }
            createdPipe = createdPipeShare.open(
                PIPE_NAME,
                SMB2ImpersonationLevel.Impersonation,
                EnumSet.of(AccessMask.GENERIC_READ, AccessMask.GENERIC_WRITE),
                EnumSet.noneOf(FileAttributes::class.java),
                SMB2ShareAccess.ALL,
                SMB2CreateDisposition.FILE_OPEN,
                null,
            )
        } catch (error: Throwable) {
            runCatching { createdPipe?.close() }
            runCatching { createdPipeShare?.close() }
            runCatching { createdSession?.close() }
            runCatching { createdConnection?.close() }
            runCatching { createdClient.close() }
            throw error
        }
        client = createdClient
        connection = checkNotNull(createdConnection)
        session = checkNotNull(createdSession)
        pipeShare = checkNotNull(createdPipeShare)
        pipe = checkNotNull(createdPipe)
    }

    override fun bind() {
        try {
            val response = pipe.transact(DceRpcCodec.bindPdu(callId++))
            DceRpcCodec.requireBindAccepted(response)
            bound = true
        } catch (error: SMBApiException) {
            throw mapSrvsvcSmbError(error)
        }
    }

    override fun call(opnum: Int, requestStub: ByteArray): ByteArray {
        try {
            check(bound) { "srvsvc transport is not bound" }
            val expectedCallId = callId++
            var response = pipe.transact(DceRpcCodec.requestPdu(expectedCallId, opnum, requestStub))
            val combined = ByteArrayOutputStream()
            var fragments = 0
            while (true) {
                val parsed = DceRpcCodec.parseResponseFragment(response, expectedCallId)
                combined.write(parsed.stub)
                fragments++
                if (parsed.last) return combined.toByteArray()
                if (fragments >= MAX_FRAGMENTS) throw SrvsvcProtocolException("Too many DCE/RPC fragments")
                response = readFragment()
            }
        } catch (error: SMBApiException) {
            throw mapSrvsvcSmbError(error)
        }
    }

    private fun readFragment(): ByteArray {
        val header = ByteArray(DceRpcCodec.HEADER_SIZE)
        readFully(header)
        val length = DceRpcCodec.fragmentLength(header)
        if (length < header.size || length > MAX_FRAGMENT_BYTES) {
            throw SrvsvcProtocolException("Invalid DCE/RPC fragment length")
        }
        val result = ByteArray(length)
        header.copyInto(result)
        if (length > header.size) readFully(result, header.size, length - header.size)
        return result
    }

    private fun readFully(destination: ByteArray, offset: Int = 0, count: Int = destination.size) {
        var position = offset
        val end = offset + count
        while (position < end) {
            val read = pipe.read(destination, position, end - position)
            if (read <= 0) throw EOFException("Truncated DCE/RPC response")
            position += read
        }
    }

    override fun close() {
        runCatching { pipe.close() }
        runCatching { pipeShare.close() }
        runCatching { session.close() }
        runCatching { connection.close() }
        runCatching { client.close() }
    }

    private companion object {
        const val IPC_SHARE = "IPC$"
        const val PIPE_NAME = "srvsvc"
        const val PIPE_WAIT_SECONDS = 10L
        const val MAX_FRAGMENTS = 64
        const val MAX_FRAGMENT_BYTES = 1 shl 20
    }
}

private fun mapSrvsvcSmbError(error: SMBApiException): Exception = when (error.statusCode) {
    NtStatus.STATUS_ACCESS_DENIED.value -> SrvsvcAccessDeniedException(error)
    NtStatus.STATUS_NOT_SUPPORTED.value,
    NtStatus.STATUS_NOT_IMPLEMENTED.value,
    NtStatus.STATUS_OBJECT_NAME_NOT_FOUND.value,
    NtStatus.STATUS_PIPE_NOT_AVAILABLE.value,
    -> SrvsvcUnsupportedException(error)
    NtStatus.STATUS_LOGON_FAILURE.value,
    NtStatus.STATUS_PASSWORD_EXPIRED.value,
    -> SourceException(SourceErrorCode.AUTHENTICATION_FAILED, "SMB authentication failed", error)
    else -> IOException(SecretRedactor.redact(error.message).ifBlank { "srvsvc transport failed" }, error)
}

internal object DceRpcCodec {
    const val HEADER_SIZE = 16
    private const val VERSION = 5
    private const val PACKET_REQUEST = 0
    private const val PACKET_RESPONSE = 2
    private const val PACKET_FAULT = 3
    private const val PACKET_BIND = 11
    private const val PACKET_BIND_ACK = 12
    private const val FLAG_FIRST = 1
    private const val FLAG_LAST = 2
    private const val DATA_REPRESENTATION_LITTLE_ENDIAN = 0x10
    private const val BIND_BODY_SIZE = 56
    private const val ERROR_ACCESS_DENIED = 5
    private const val NCA_S_FAULT_ACCESS_DENIED = 0x1C00001C
    private const val NCA_S_OP_RNG_ERROR = 0x1C010002
    private val SRVSVC_SYNTAX = byteArrayOf(
        0xC8.toByte(), 0x4F, 0x32, 0x4B, 0x70, 0x16, 0xD3.toByte(), 0x01,
        0x12, 0x78, 0x5A, 0x47, 0xBF.toByte(), 0x6E, 0xE1.toByte(), 0x88.toByte(),
    )
    private val NDR_SYNTAX = byteArrayOf(
        0x04, 0x5D, 0x88.toByte(), 0x8A.toByte(), 0xEB.toByte(), 0x1C, 0xC9.toByte(), 0x11,
        0x9F.toByte(), 0xE8.toByte(), 0x08, 0x00, 0x2B, 0x10, 0x48, 0x60,
    )

    data class ResponseFragment(val stub: ByteArray, val last: Boolean)

    fun bindPdu(callId: Int): ByteArray {
        val body = ByteBuffer.allocate(BIND_BODY_SIZE).order(ByteOrder.LITTLE_ENDIAN)
        body.putShort(0xFFFF.toShort())
        body.putShort(0xFFFF.toShort())
        body.putInt(0)
        body.put(1)
        body.put(0)
        body.putShort(0)
        body.putShort(0)
        body.put(1)
        body.put(0)
        body.put(SRVSVC_SYNTAX)
        body.putShort(3)
        body.putShort(0)
        body.put(NDR_SYNTAX)
        body.putInt(2)
        return pdu(PACKET_BIND, FLAG_FIRST or FLAG_LAST, callId, body.array())
    }

    fun requestPdu(callId: Int, opnum: Int, stub: ByteArray): ByteArray {
        require(opnum in 0..0xFFFF) { "Invalid DCE/RPC operation" }
        val body = ByteBuffer.allocate(8 + stub.size).order(ByteOrder.LITTLE_ENDIAN)
        body.putInt(stub.size)
        body.putShort(0)
        body.putShort(opnum.toShort())
        body.put(stub)
        return pdu(PACKET_REQUEST, FLAG_FIRST or FLAG_LAST, callId, body.array())
    }

    fun requireBindAccepted(packet: ByteArray) {
        val header = parseHeader(packet)
        if (header.packetType == PACKET_FAULT) throw fault(packet)
        if (header.packetType != PACKET_BIND_ACK) throw SrvsvcProtocolException("Expected a DCE/RPC bind acknowledgement")
        var offset = HEADER_SIZE + 8
        requireAvailable(packet, offset, 2)
        val secondaryAddressLength = u16(packet, offset)
        offset += 2
        requireAvailable(packet, offset, secondaryAddressLength)
        offset += secondaryAddressLength
        offset = align4(offset)
        requireAvailable(packet, offset, 8)
        val resultCount = packet[offset].toInt() and 0xFF
        offset += 4
        if (resultCount < 1) throw SrvsvcProtocolException("DCE/RPC bind returned no context result")
        val result = u16(packet, offset)
        val reason = u16(packet, offset + 2)
        if (result != 0) throw SrvsvcUnsupportedException(SrvsvcProtocolException("DCE/RPC bind rejected: $result/$reason"))
    }

    fun parseResponseFragment(packet: ByteArray, expectedCallId: Int): ResponseFragment {
        val header = parseHeader(packet)
        if (header.callId != expectedCallId) throw SrvsvcProtocolException("DCE/RPC call id mismatch")
        if (header.packetType == PACKET_FAULT) throw fault(packet)
        if (header.packetType != PACKET_RESPONSE) throw SrvsvcProtocolException("Expected a DCE/RPC response")
        requireAvailable(packet, HEADER_SIZE, 8)
        val stubEnd = header.fragmentLength - header.authLength
        if (stubEnd < HEADER_SIZE + 8) throw SrvsvcProtocolException("Invalid DCE/RPC response body")
        return ResponseFragment(
            packet.copyOfRange(HEADER_SIZE + 8, stubEnd),
            header.flags and FLAG_LAST != 0,
        )
    }

    fun fragmentLength(header: ByteArray): Int {
        requireAvailable(header, 0, HEADER_SIZE)
        return u16(header, 8)
    }

    private fun pdu(packetType: Int, flags: Int, callId: Int, body: ByteArray): ByteArray {
        val length = HEADER_SIZE + body.size
        require(length <= 0xFFFF) { "DCE/RPC request is too large" }
        return ByteBuffer.allocate(length).order(ByteOrder.LITTLE_ENDIAN).apply {
            put(VERSION.toByte())
            put(0)
            put(packetType.toByte())
            put(flags.toByte())
            put(DATA_REPRESENTATION_LITTLE_ENDIAN.toByte())
            put(0)
            put(0)
            put(0)
            putShort(length.toShort())
            putShort(0)
            putInt(callId)
            put(body)
        }.array()
    }

    private fun parseHeader(packet: ByteArray): Header {
        requireAvailable(packet, 0, HEADER_SIZE)
        if ((packet[0].toInt() and 0xFF) != VERSION || (packet[4].toInt() and 0xFF) != DATA_REPRESENTATION_LITTLE_ENDIAN) {
            throw SrvsvcProtocolException("Unsupported DCE/RPC header")
        }
        val fragmentLength = u16(packet, 8)
        val authLength = u16(packet, 10)
        if (fragmentLength != packet.size || authLength > fragmentLength - HEADER_SIZE) {
            throw SrvsvcProtocolException("Invalid DCE/RPC fragment")
        }
        return Header(
            packetType = packet[2].toInt() and 0xFF,
            flags = packet[3].toInt() and 0xFF,
            fragmentLength = fragmentLength,
            authLength = authLength,
            callId = i32(packet, 12),
        )
    }

    private fun fault(packet: ByteArray): IOException {
        requireAvailable(packet, HEADER_SIZE, 12)
        return when (val status = i32(packet, HEADER_SIZE + 8)) {
            ERROR_ACCESS_DENIED, NCA_S_FAULT_ACCESS_DENIED -> SrvsvcAccessDeniedException()
            NCA_S_OP_RNG_ERROR -> SrvsvcUnsupportedException()
            else -> SrvsvcProtocolException("DCE/RPC fault 0x${status.toUInt().toString(16)}")
        }
    }

    private data class Header(
        val packetType: Int,
        val flags: Int,
        val fragmentLength: Int,
        val authLength: Int,
        val callId: Int,
    )
}

internal object SrvsvcNdrCodec {
    private const val LEVEL_1 = 1
    private const val PREFERRED_MAX_BYTES = 60 * 1024
    private const val MAX_SHARE_COUNT = 16_384
    private const val MAX_STRING_CHARS = 32_768
    private const val REFERENT_SERVER = 0x0002_0000
    private const val REFERENT_CONTAINER = 0x0002_0004
    private const val REFERENT_RESUME = 0x0002_0008

    data class Share(val name: String, val type: Int, val remark: String?)
    data class ShareEnumResponse(val shares: List<Share>, val resumeHandle: Int?, val status: Int)

    fun encodeShareEnumRequest(server: String, resumeHandle: Int?): ByteArray {
        require(server.isNotBlank() && '\u0000' !in server) { "Invalid SMB server" }
        val writer = NdrWriter()
        writer.i32(REFERENT_SERVER)
        writer.wideString("\\\\$server")
        writer.i32(LEVEL_1)
        writer.i32(LEVEL_1)
        writer.i32(REFERENT_CONTAINER)
        writer.i32(0)
        writer.i32(0)
        writer.i32(PREFERRED_MAX_BYTES)
        if (resumeHandle == null) {
            writer.i32(0)
        } else {
            writer.i32(REFERENT_RESUME)
            writer.i32(resumeHandle)
        }
        return writer.toByteArray()
    }

    fun decodeShareEnumResponse(stub: ByteArray): ShareEnumResponse {
        val reader = NdrReader(stub)
        val level = reader.i32()
        val discriminator = reader.i32()
        if (level != LEVEL_1 || discriminator != LEVEL_1) throw SrvsvcUnsupportedException()
        val containerPointer = reader.i32()
        val shares = if (containerPointer == 0) {
            emptyList()
        } else {
            val entriesRead = reader.i32()
            val bufferPointer = reader.i32()
            if (entriesRead !in 0..MAX_SHARE_COUNT) throw SrvsvcProtocolException("Invalid srvsvc share count")
            if (entriesRead == 0 || bufferPointer == 0) {
                if (entriesRead != 0 || bufferPointer != 0) throw SrvsvcProtocolException("Invalid srvsvc share buffer")
                emptyList()
            } else {
                val conformantCount = reader.i32()
                if (conformantCount != entriesRead) throw SrvsvcProtocolException("srvsvc share count mismatch")
                val raw = ArrayList<RawShare>(entriesRead)
                repeat(entriesRead) {
                    raw += RawShare(reader.i32(), reader.i32(), reader.i32())
                }
                raw.map { value ->
                    val name = if (value.namePointer == 0) "" else reader.wideString()
                    val remark = if (value.remarkPointer == 0) null else reader.wideString()
                    if (name.isBlank()) throw SrvsvcProtocolException("srvsvc returned an empty share name")
                    Share(name, value.type, remark)
                }
            }
        }
        val totalEntries = reader.i32()
        if (totalEntries < shares.size) throw SrvsvcProtocolException("Invalid srvsvc total entry count")
        val resumePointer = reader.i32()
        val resume = if (resumePointer == 0) null else reader.i32()
        val status = reader.i32()
        if (reader.remaining != 0) throw SrvsvcProtocolException("Trailing srvsvc response bytes")
        return ShareEnumResponse(shares, resume, status)
    }

    private data class RawShare(val namePointer: Int, val type: Int, val remarkPointer: Int)

    private class NdrWriter {
        private val output = ByteArrayOutputStream()

        fun i32(value: Int) {
            align4()
            output.write(value and 0xFF)
            output.write(value ushr 8 and 0xFF)
            output.write(value ushr 16 and 0xFF)
            output.write(value ushr 24 and 0xFF)
        }

        fun wideString(value: String) {
            val terminated = "$value\u0000"
            i32(terminated.length)
            i32(0)
            i32(terminated.length)
            terminated.forEach { character ->
                output.write(character.code and 0xFF)
                output.write(character.code ushr 8 and 0xFF)
            }
            align4()
        }

        fun toByteArray(): ByteArray = output.toByteArray()

        private fun align4() {
            while (output.size() and 3 != 0) output.write(0)
        }
    }

    private class NdrReader(private val bytes: ByteArray) {
        private var position = 0
        val remaining: Int get() = bytes.size - position

        fun i32(): Int {
            align4()
            requireAvailable(bytes, position, 4)
            return i32(bytes, position).also { position += 4 }
        }

        fun wideString(): String {
            val maximumCount = i32()
            val offset = i32()
            val actualCount = i32()
            if (maximumCount !in 1..MAX_STRING_CHARS || offset != 0 || actualCount !in 1..maximumCount) {
                throw SrvsvcProtocolException("Invalid NDR string bounds")
            }
            requireAvailable(bytes, position, actualCount * 2)
            val builder = StringBuilder(actualCount)
            repeat(actualCount) {
                val value = u16(bytes, position)
                position += 2
                if (it < actualCount - 1) builder.append(value.toChar())
                else if (value != 0) throw SrvsvcProtocolException("NDR string is not terminated")
            }
            align4()
            return builder.toString()
        }

        private fun align4() {
            position = align4(position)
            if (position > bytes.size) throw SrvsvcProtocolException("Truncated NDR data")
        }
    }
}

private fun align4(value: Int): Int = (value + 3) and -4

private fun requireAvailable(bytes: ByteArray, offset: Int, count: Int) {
    if (offset < 0 || count < 0 || offset > bytes.size - count) throw SrvsvcProtocolException("Truncated binary response")
}

private fun u16(bytes: ByteArray, offset: Int): Int {
    requireAvailable(bytes, offset, 2)
    return (bytes[offset].toInt() and 0xFF) or ((bytes[offset + 1].toInt() and 0xFF) shl 8)
}

private fun i32(bytes: ByteArray, offset: Int): Int {
    requireAvailable(bytes, offset, 4)
    return (bytes[offset].toInt() and 0xFF) or
        ((bytes[offset + 1].toInt() and 0xFF) shl 8) or
        ((bytes[offset + 2].toInt() and 0xFF) shl 16) or
        (bytes[offset + 3].toInt() shl 24)
}
