package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayOutputStream

class SmbPlatformTest {
    @Test
    fun normalizationRejectsTraversalAndSmbOne() {
        assertThat(SmbPathNormalizer.normalizePath("folder/sub/file.cbz")).isEqualTo("folder\\sub\\file.cbz")
        assertThrows(IllegalArgumentException::class.java) { SmbPathNormalizer.normalizePath("folder/../secret") }
        assertThrows(IllegalArgumentException::class.java) { SmbPathNormalizer.normalizeShare("public/data") }
        assertThat(
            SmbPathNormalizer.sameEntry(
                SmbLocation("Comics", "Books\\One.cbz"),
                SmbLocation("comics", "books/one.cbz"),
            ),
        ).isTrue()
        assertThrows(IllegalArgumentException::class.java) { SmbDialectPolicy.requireSmb2Or3("SMB_1") }
        SmbDialectPolicy.requireSmb2Or3("SMB_3_1_1")
        assertThat(SmbConnectionSupport.config(
            SmbProfile("prefer-encryption", "nas", authenticationMode = SmbAuthenticationMode.GUEST, requireEncryption = false),
        ).isEncryptData).isTrue()
    }

    @Test
    fun ndrRequestCarriesServerAndResumeHandle() {
        val request = SrvsvcNdrCodec.encodeShareEnumRequest("nas", 42)
        assertThat(request.size).isGreaterThan(40)
        assertThat(readLeInt(request, request.size - 4)).isEqualTo(42)
    }

    @Test
    fun srvsvcFixtureReturnsOnlyDiskShares() = runTest {
        var bound = false
        var capturedOpnum = -1
        val service = SrvsvcShareEnumerationService(SrvsvcRpcTransportFactory {
            object : SrvsvcRpcTransport {
                override fun bind() { bound = true }
                override fun call(opnum: Int, requestStub: ByteArray): ByteArray {
                    capturedOpnum = opnum
                    return shareResponseFixture()
                }
                override fun close() = Unit
            }
        })
        val result = service.enumerate(SmbProfile("profile", "nas", authenticationMode = SmbAuthenticationMode.GUEST))
        assertThat(bound).isTrue()
        assertThat(capturedOpnum).isEqualTo(15)
        assertThat(result).isEqualTo(ShareDiscoveryResult.Shares(listOf("Books", "C$")))
    }

    @Test
    fun onlyAccessDeniedOrUnsupportedUseManualFallback() = runTest {
        val denied = SrvsvcShareEnumerationService(SrvsvcRpcTransportFactory {
            object : SrvsvcRpcTransport {
                override fun bind() = throw SrvsvcAccessDeniedException()
                override fun call(opnum: Int, requestStub: ByteArray) = error("unreachable")
                override fun close() = Unit
            }
        })
        assertThat(denied.enumerate(SmbProfile("p1", "nas", authenticationMode = SmbAuthenticationMode.GUEST)))
            .isEqualTo(ShareDiscoveryResult.ManualShareRequired("SRVSVC_ACCESS_DENIED"))
    }

    private fun shareResponseFixture(): ByteArray = ByteArrayOutputStream().apply {
        le32(1); le32(1); le32(0x20000); le32(3); le32(0x20004); le32(3)
        le32(0x20008); le32(0); le32(0)
        le32(0x2000C); le32(3); le32(0)
        le32(0x20010); le32(0x80000000.toInt()); le32(0x20014)
        ndrString("Books"); ndrString("IPC$"); ndrString("C$"); ndrString("Administrative share")
        le32(3); le32(0); le32(0)
    }.toByteArray()

    private fun ByteArrayOutputStream.le32(value: Int) {
        write(value and 0xFF); write(value ushr 8 and 0xFF); write(value ushr 16 and 0xFF); write(value ushr 24 and 0xFF)
    }

    private fun ByteArrayOutputStream.ndrString(value: String) {
        val text = "$value\u0000"
        le32(text.length); le32(0); le32(text.length)
        text.forEach { write(it.code and 0xFF); write(it.code ushr 8 and 0xFF) }
        while (size() and 3 != 0) write(0)
    }

    private fun readLeInt(bytes: ByteArray, offset: Int): Int =
        (bytes[offset].toInt() and 0xFF) or ((bytes[offset + 1].toInt() and 0xFF) shl 8) or
            ((bytes[offset + 2].toInt() and 0xFF) shl 16) or (bytes[offset + 3].toInt() shl 24)
}
