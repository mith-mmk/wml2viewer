package io.github.mith_mmk.wml2viewer.platform.smb

import com.hierynomus.msdtyp.AccessMask
import com.hierynomus.mserref.NtStatus
import com.hierynomus.msfscc.FileAttributes
import com.hierynomus.mssmb2.SMB2CreateDisposition
import com.hierynomus.mssmb2.SMB2ShareAccess
import com.hierynomus.mssmb2.SMBApiException
import com.hierynomus.smbj.SMBClient
import com.hierynomus.smbj.connection.Connection
import com.hierynomus.smbj.session.Session
import com.hierynomus.smbj.share.DiskEntry
import com.hierynomus.smbj.share.DiskShare
import com.hierynomus.smbj.share.File as SmbFile
import io.github.mith_mmk.wml2viewer.data.source.AtomicWriteSession
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.CollisionResolution
import io.github.mith_mmk.wml2viewer.data.source.CollisionResolver
import io.github.mith_mmk.wml2viewer.data.source.CreateRequest
import io.github.mith_mmk.wml2viewer.data.source.DeleteDisposition
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceCapabilities
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.data.source.SourceProvider
import io.github.mith_mmk.wml2viewer.data.source.SourceRead
import io.github.mith_mmk.wml2viewer.data.source.SourceThumbnail
import io.github.mith_mmk.wml2viewer.data.source.WriteVerification
import io.github.mith_mmk.wml2viewer.data.source.sha256AndSize
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore
import io.github.mith_mmk.wml2viewer.platform.security.SecretRedactor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.FilterInputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.URLConnection
import java.security.MessageDigest
import java.util.EnumSet
import java.util.UUID

/** Low-level SMB2/3 provider. The incomplete SMBJ NIO provider is deliberately not used. */
class SmbSourceProvider(
    private val profile: SmbProfile,
    private val credentialStore: CredentialStore,
    private val shareEnumerationService: SmbShareEnumerationService = SrvsvcShareEnumerationService(credentialStore),
) : SourceProvider {
    private val mutableSecurityStatus = MutableStateFlow(SmbSecurityStatus.DISCONNECTED)
    private val connectedShares = SharedCloseableCache<String, ConnectedShare>(::connectNew)
    val securityStatus: StateFlow<SmbSecurityStatus> = mutableSecurityStatus.asStateFlow()
    override val providerId = "smb:${profile.profileId}"
    override val root = ref(SmbLocation(profile.share, ""))
    override val capabilities = SourceCapabilities(
        canList = true,
        canRead = true,
        canCreate = true,
        canCopyWithinProvider = true,
        canMoveWithinProvider = true,
        canRename = true,
        canTrash = false,
        canDelete = true,
        canThumbnail = false,
        hasAtomicFinalize = true,
        canCopyDirectoriesWithinProvider = false,
        canMoveDirectoriesWithinProvider = true,
        canTransferDirectoriesAcrossProviders = true,
    )

    override suspend fun list(parent: EntryRef): List<SourceEntry> {
        val location = location(parent)
        if (location.isServerRoot) return listShares(parent)
        return retryNetwork {
            withShare(location.requireShare()) { share ->
                share.list(location.path)
                    .asSequence()
                    .filterNot { it.fileName == "." || it.fileName == ".." }
                    .map { information ->
                        val childPath = SmbPathNormalizer.child(location.path, information.fileName)
                        val directory = information.fileAttributes.has(FileAttributes.FILE_ATTRIBUTE_DIRECTORY)
                        SourceEntry(
                            ref(SmbLocation(location.share, childPath)),
                            parent,
                            information.fileName,
                            if (directory) EntryKind.DIRECTORY else EntryKind.FILE,
                            if (directory) null else mimeType(information.fileName),
                            if (directory) null else information.endOfFile,
                            information.lastWriteTime.toEpochMillis(),
                            information.fileAttributes.has(FileAttributes.FILE_ATTRIBUTE_HIDDEN),
                            smbEntryCapabilities(if (directory) EntryKind.DIRECTORY else EntryKind.FILE, isRoot = false),
                            information.fileAttributes,
                        )
                    }
                    .sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
                    .toList()
            }
        }
    }

    override suspend fun stat(entry: EntryRef): SourceEntry {
        val location = location(entry)
        if (location.path.isEmpty()) {
            val share = location.share ?: return SourceEntry(
                entry,
                null,
                profile.server,
                EntryKind.DIRECTORY,
                null,
                null,
                null,
                effectiveCapabilities = SourceCapabilities(canList = true, canRead = false),
            )
            return SourceEntry(
                entry,
                if (profile.share == null) root else null,
                share,
                EntryKind.DIRECTORY,
                null,
                null,
                null,
                effectiveCapabilities = smbEntryCapabilities(EntryKind.DIRECTORY, isRoot = true),
            )
        }
        return retryNetwork {
            withShare(location.requireShare()) { share ->
                val info = share.getFileInformation(location.path)
                val attributes = info.basicInformation.fileAttributes
                val directory = attributes.has(FileAttributes.FILE_ATTRIBUTE_DIRECTORY)
                SourceEntry(
                    entry,
                    parentRef(location),
                    SmbPathNormalizer.name(location.path),
                    if (directory) EntryKind.DIRECTORY else EntryKind.FILE,
                    if (directory) null else mimeType(location.path),
                    if (directory) null else info.standardInformation.endOfFile,
                    info.basicInformation.lastWriteTime.toEpochMillis(),
                    attributes.has(FileAttributes.FILE_ATTRIBUTE_HIDDEN),
                    smbEntryCapabilities(if (directory) EntryKind.DIRECTORY else EntryKind.FILE, isRoot = false),
                    attributes,
                )
            }
        }
    }

    override suspend fun openRead(entry: EntryRef): SourceRead {
        val info = stat(entry)
        if (info.kind != EntryKind.FILE) throw SourceException(SourceErrorCode.UNSUPPORTED, "Cannot read an SMB directory")
        val location = location(entry)
        return retryNetwork {
            val connected = connectedShares.acquire(location.requireShare())
            try {
                val file = connected.value.share.openFile(
                    location.path,
                    EnumSet.of(AccessMask.FILE_READ_DATA, AccessMask.FILE_READ_ATTRIBUTES),
                    null,
                    SMB2ShareAccess.ALL,
                    SMB2CreateDisposition.FILE_OPEN,
                    null,
                )
                SourceRead(SmbReadStream(file.inputStream, file, connected), info.size, info.mimeType)
            } catch (error: Throwable) {
                connected.close()
                throw error
            }
        }
    }

    override suspend fun create(parent: EntryRef, request: CreateRequest): AtomicWriteSession {
        if (request.kind != EntryKind.FILE) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Use createDirectory for directories")
        }
        val parentLocation = location(parent)
        val shareName = parentLocation.requireShare()
        val children = list(parent).associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(request.name, request.collisionPolicy) { it in children }) {
            CollisionResolution.Skip -> SmbSkippedWriteSession(children.getValue(request.name).ref, request.name)
            is CollisionResolution.Use -> retryNetwork {
                val tempPath = SmbPathNormalizer.child(
                    parentLocation.path,
                    ".wml2viewer-${UUID.randomUUID()}.part",
                )
                val connected = connectedShares.acquire(shareName)
                try {
                    val file = connected.value.share.openFile(
                        tempPath,
                        EnumSet.of(AccessMask.FILE_WRITE_DATA, AccessMask.FILE_WRITE_ATTRIBUTES, AccessMask.DELETE),
                        EnumSet.of(FileAttributes.FILE_ATTRIBUTE_TEMPORARY),
                        SMB2ShareAccess.ALL,
                        SMB2CreateDisposition.FILE_CREATE,
                        null,
                    )
                    SmbAtomicWriteSession(
                        provider = this,
                        temporaryRef = ref(SmbLocation(shareName, tempPath)),
                        plannedFinalName = resolution.name,
                        finalPath = SmbPathNormalizer.child(parentLocation.path, resolution.name),
                        replacedRef = if (resolution.replaceExisting) children[request.name]?.ref else null,
                        connected = connected,
                        file = file,
                    )
                } catch (error: Throwable) {
                    connected.close()
                    throw error
                }
            }
        }
    }

    override suspend fun createDirectory(
        parent: EntryRef,
        name: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef {
        val parentLocation = location(parent)
        val shareName = parentLocation.requireShare()
        val prepared = prepareDestination(parent, name, collisionPolicy)
            ?: return findByName(parent, name)!!.ref
        val temporaryPath = SmbPathNormalizer.child(
            parentLocation.path,
            ".wml2viewer-${UUID.randomUUID()}.dir",
        )
        try {
            retryNetwork { withShare(shareName) { it.mkdir(temporaryPath) } }
            return commitRename(
                ref(SmbLocation(shareName, temporaryPath)),
                SmbPathNormalizer.child(parentLocation.path, prepared.name),
                prepared.replaced,
            )
        } catch (error: Throwable) {
            runCatching { abortTemporary(ref(SmbLocation(shareName, temporaryPath))) }
            throw error
        }
    }

    override suspend fun copy(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef {
        val sourceInfo = stat(source)
        if (sourceInfo.kind != EntryKind.FILE) throw SourceException(SourceErrorCode.UNSUPPORTED, "Directory copy is not supported")
        val pending = create(
            destinationParent,
            CreateRequest(destinationName, sourceInfo.mimeType ?: "application/octet-stream", collisionPolicy),
        )
        pending.skippedExistingRef?.let {
            pending.abort()
            return it
        }
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            var bytes = 0L
            openRead(source).use { read ->
                read.stream.use { input ->
                    pending.output.use { output ->
                        val buffer = ByteArray(COPY_BUFFER_SIZE)
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            output.write(buffer, 0, count)
                            digest.update(buffer, 0, count)
                            bytes += count
                        }
                    }
                }
            }
            val expected = WriteVerification(bytes, digest.digest().joinToString("") { "%02x".format(it) })
            if (!pending.verify(expected)) throw SourceException(SourceErrorCode.INTEGRITY, "SMB copy verification failed")
            return pending.commit()
        } catch (error: Throwable) {
            pending.abort()
            throw error
        }
    }

    override suspend fun move(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef {
        val sourceLocation = location(source)
        val destinationLocation = location(destinationParent)
        val requestedDestination = SmbLocation(
            destinationLocation.share,
            SmbPathNormalizer.child(destinationLocation.path, destinationName),
        )
        if (SmbPathNormalizer.sameEntry(sourceLocation, requestedDestination)) return source
        if (sourceLocation.share != destinationLocation.share) {
            val copied = copy(source, destinationParent, destinationName, collisionPolicy)
            trashOrDelete(source, allowPermanentDelete = true)
            return copied
        }
        val prepared = prepareDestination(destinationParent, destinationName, collisionPolicy) ?: return findByName(destinationParent, destinationName)!!.ref
        val finalPath = SmbPathNormalizer.child(destinationLocation.path, prepared.name)
        return commitRename(source, finalPath, prepared.replaced)
    }

    override suspend fun rename(entry: EntryRef, newName: String, collisionPolicy: CollisionPolicy): EntryRef {
        val source = location(entry)
        val parentPath = SmbPathNormalizer.parent(source.path)
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SMB share root cannot be renamed")
        if (
            SmbPathNormalizer.sameEntry(
                source,
                SmbLocation(source.share, SmbPathNormalizer.child(parentPath, newName)),
            )
        ) return entry
        val parent = ref(SmbLocation(source.share, parentPath))
        val prepared = prepareDestination(parent, newName, collisionPolicy) ?: return findByName(parent, newName)!!.ref
        return commitRename(entry, SmbPathNormalizer.child(parentPath, prepared.name), prepared.replaced)
    }

    override suspend fun trashOrDelete(entry: EntryRef, allowPermanentDelete: Boolean): DeleteDisposition {
        if (!allowPermanentDelete) throw SourceException(SourceErrorCode.UNSUPPORTED, "SMB has no portable trash operation")
        val location = location(entry)
        if (location.path.isEmpty()) throw SourceException(SourceErrorCode.UNSUPPORTED, "The SMB share root cannot be deleted")
        val directory = stat(entry).kind == EntryKind.DIRECTORY
        retryNetwork {
            withShare(location.requireShare()) { share ->
                if (directory) share.rmdir(location.path, false) else share.rm(location.path)
            }
        }
        return DeleteDisposition.PERMANENTLY_DELETED
    }

    override suspend fun thumbnail(entry: EntryRef, maxWidth: Int, maxHeight: Int): SourceThumbnail? = null

    override fun close() {
        connectedShares.close()
        mutableSecurityStatus.value = SmbSecurityStatus.DISCONNECTED
    }

    internal suspend fun verifyTemporary(ref: EntryRef, expected: WriteVerification): Boolean =
        openRead(ref).use { it.stream.sha256AndSize() == expected }

    internal suspend fun abortTemporary(ref: EntryRef) {
        runCatching { trashOrDelete(ref, allowPermanentDelete = true) }
    }

    internal suspend fun commitTemporary(temp: EntryRef, finalPath: String, replaced: EntryRef?): EntryRef =
        commitRename(temp, finalPath, replaced)

    private suspend fun commitRename(source: EntryRef, finalPath: String, replaced: EntryRef?): EntryRef {
        val sourceLocation = location(source)
        // SMB2 FILE_RENAME_INFORMATION performs the replacement server-side as one
        // rename operation, avoiding a process-death gap between backup and finalize.
        renamePath(sourceLocation, finalPath, replace = replaced != null)
        return ref(SmbLocation(sourceLocation.share, finalPath))
    }

    private suspend fun renamePath(source: SmbLocation, targetPath: String, replace: Boolean) {
        retryNetwork {
            withShare(source.requireShare()) { share ->
                share.open(
                    source.path,
                    EnumSet.of(AccessMask.DELETE, AccessMask.FILE_READ_ATTRIBUTES),
                    null,
                    SMB2ShareAccess.ALL,
                    SMB2CreateDisposition.FILE_OPEN,
                    null,
                ).use { entry -> entry.rename(targetPath, replace) }
            }
        }
    }

    private suspend fun prepareDestination(
        parent: EntryRef,
        name: String,
        collisionPolicy: CollisionPolicy,
    ): PreparedDestination? {
        val children = list(parent)
        val byName = children.associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(name, collisionPolicy) { it in byName }) {
            CollisionResolution.Skip -> null
            is CollisionResolution.Use -> PreparedDestination(
                resolution.name,
                if (resolution.replaceExisting) byName[name]?.ref else null,
            )
        }
    }

    private suspend fun findByName(parent: EntryRef, name: String) = list(parent).firstOrNull { it.name == name }

    private suspend fun listShares(parent: EntryRef): List<SourceEntry> = when (val discovery = shareEnumerationService.enumerate(profile)) {
        is ShareDiscoveryResult.ManualShareRequired -> throw SourceException(SourceErrorCode.UNSUPPORTED, discovery.reason)
        is ShareDiscoveryResult.Shares -> discovery.names
            .map(SmbPathNormalizer::normalizeShare)
            .distinctBy { it.lowercase() }
            .sortedWith(String.CASE_INSENSITIVE_ORDER)
            .map { name ->
                SourceEntry(
                    ref(SmbLocation(name, "")),
                    parent,
                    name,
                    EntryKind.DIRECTORY,
                    null,
                    null,
                    null,
                    effectiveCapabilities = smbEntryCapabilities(EntryKind.DIRECTORY, isRoot = true),
                )
            }
    }

    private fun connectNew(shareName: String): ConnectedShare {
        val client = SMBClient(SmbConnectionSupport.config(profile))
        try {
            val connection = client.connect(profile.server, profile.port)
            val session = SmbConnectionSupport.authenticate(connection, profile, credentialStore)
            val status = SmbConnectionSupport.securityStatus(connection, session)
            if (profile.requireEncryption && !status.encryptionActive) {
                throw SourceException(SourceErrorCode.ACCESS_DENIED, "SMB encryption is required but was not negotiated")
            }
            mutableSecurityStatus.value = status
            val share = session.connectShare(shareName) as? DiskShare
                ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SMB share is not a disk share")
            return ConnectedShare(client, connection, session, share)
        } catch (error: Throwable) {
            runCatching { client.close() }
            throw error
        }
    }

    private fun <T> withShare(shareName: String, block: (DiskShare) -> T): T =
        connectedShares.acquire(shareName).use { block(it.value.share) }

    private suspend fun <T> retryNetwork(block: () -> T): T = withContext(Dispatchers.IO) {
        var last: SourceException? = null
        repeat(MAX_ATTEMPTS) { attempt ->
            try {
                return@withContext block()
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                val mapped = mapSmbError(error)
                last = mapped
                if (!mapped.retryable || attempt == MAX_ATTEMPTS - 1) throw mapped
                connectedShares.invalidateAll()
                delay(RETRY_DELAYS_MS[attempt])
            }
        }
        throw last ?: SourceException(SourceErrorCode.NETWORK, "SMB request failed")
    }

    private fun mapSmbError(error: Throwable): SourceException {
        if (error is SourceException) return error
        val message = SecretRedactor.redact(error.message).ifBlank { error.javaClass.simpleName }
        if (error is SMBApiException) {
            val code = when (error.statusCode) {
                NtStatus.STATUS_LOGON_FAILURE.value,
                NtStatus.STATUS_PASSWORD_EXPIRED.value,
                -> SourceErrorCode.AUTHENTICATION_FAILED
                NtStatus.STATUS_ACCESS_DENIED.value -> SourceErrorCode.ACCESS_DENIED
                NtStatus.STATUS_OBJECT_NAME_NOT_FOUND.value,
                NtStatus.STATUS_OBJECT_PATH_NOT_FOUND.value,
                -> SourceErrorCode.NOT_FOUND
                NtStatus.STATUS_OBJECT_NAME_COLLISION.value -> SourceErrorCode.ALREADY_EXISTS
                else -> SourceErrorCode.IO
            }
            return SourceException(code, message, error, retryable = false)
        }
        return if (error.isRetryableSmbNetworkFailure()) {
            SourceException(SourceErrorCode.NETWORK, message, error, retryable = true)
        } else {
            SourceException(SourceErrorCode.IO, message, error, retryable = false)
        }
    }

    private fun parentRef(location: SmbLocation): EntryRef? =
        SmbPathNormalizer.parent(location.path)?.let { ref(SmbLocation(location.share, it)) }

    private fun location(ref: EntryRef): SmbLocation = try {
        ref.smbLocation(providerId)
    } catch (error: IllegalArgumentException) {
        throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Malformed SMB entry reference", error)
    }

    private fun ref(location: SmbLocation) = EntryRef(providerId, SmbOpaqueIdCodec.encode(location))

    private fun SmbLocation.requireShare(): String = share
        ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "Select or enter an SMB share first")

    private fun mimeType(path: String): String? = URLConnection.guessContentTypeFromName(path)

    private fun smbEntryCapabilities(kind: EntryKind, isRoot: Boolean): SourceCapabilities = SourceCapabilities(
        canList = kind == EntryKind.DIRECTORY,
        canRead = kind == EntryKind.FILE,
        canCreate = kind == EntryKind.DIRECTORY,
        canCopyWithinProvider = kind == EntryKind.FILE,
        canMoveWithinProvider = !isRoot,
        canRename = !isRoot,
        canTrash = false,
        canDelete = !isRoot,
        canThumbnail = false,
        hasAtomicFinalize = true,
        canCopyDirectoriesWithinProvider = false,
        canMoveDirectoriesWithinProvider = kind == EntryKind.DIRECTORY && !isRoot,
        canTransferDirectoriesAcrossProviders = kind == EntryKind.DIRECTORY,
    )

    private fun Long.has(attribute: FileAttributes): Boolean = this and attribute.value != 0L

    private data class PreparedDestination(val name: String, val replaced: EntryRef?)

    companion object {
        private const val MAX_ATTEMPTS = 3
        private val RETRY_DELAYS_MS = longArrayOf(250, 750, 1_500)
        private const val COPY_BUFFER_SIZE = 256 * 1024
    }
}

private class ConnectedShare(
    private val client: SMBClient,
    private val connection: Connection,
    private val session: Session,
    val share: DiskShare,
) : AutoCloseable {
    override fun close() {
        runCatching { share.close() }
        runCatching { session.close() }
        runCatching { connection.close() }
        runCatching { client.close() }
    }
}

private class SmbReadStream(
    input: InputStream,
    private val file: SmbFile,
    private val connected: SharedCloseableCache.Lease<ConnectedShare>,
) : FilterInputStream(input) {
    override fun close() {
        try {
            super.close()
        } finally {
            runCatching { file.close() }
            connected.close()
        }
    }
}

private class SmbSkippedWriteSession(
    private val existing: EntryRef,
    override val plannedFinalName: String,
) : AtomicWriteSession {
    override val temporaryRef = existing
    override val replacementBackupName: String? = null
    override val skippedExistingRef = existing
    override val output = object : OutputStream() { override fun write(value: Int) = Unit }
    override suspend fun verify(expected: WriteVerification) = true
    override suspend fun commit() = existing
    override suspend fun abort() = Unit
}

private class SmbAtomicWriteSession(
    private val provider: SmbSourceProvider,
    override val temporaryRef: EntryRef,
    override val plannedFinalName: String,
    private val finalPath: String,
    private val replacedRef: EntryRef?,
    private val connected: SharedCloseableCache.Lease<ConnectedShare>,
    private val file: SmbFile,
) : AtomicWriteSession {
    override val replacementBackupName: String? = null
    override val output: OutputStream = file.outputStream
    private var resourcesClosed = false
    private var verified = false
    private var finished = false

    override suspend fun verify(expected: WriteVerification): Boolean {
        check(!finished) { "Write session is finished" }
        closeResources()
        return provider.verifyTemporary(temporaryRef, expected).also { verified = it }
    }

    override suspend fun commit(): EntryRef {
        check(!finished) { "Write session is finished" }
        check(verified) { "Write must be verified before commit" }
        return provider.commitTemporary(temporaryRef, finalPath, replacedRef).also { finished = true }
    }

    override suspend fun abort() {
        if (finished) return
        finished = true
        closeResources()
        provider.abortTemporary(temporaryRef)
    }

    private fun closeResources() {
        if (resourcesClosed) return
        resourcesClosed = true
        runCatching { output.close() }
        runCatching { file.close() }
        connected.close()
    }
}
