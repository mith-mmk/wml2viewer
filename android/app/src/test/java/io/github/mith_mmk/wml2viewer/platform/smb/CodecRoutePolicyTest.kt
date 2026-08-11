package io.github.mith_mmk.wml2viewer.platform.codec

import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Test
import java.nio.ByteBuffer

class CodecRoutePolicyTest {
    @Test
    fun defaultAndOverridesResolveAllRoutes() {
        val policy = CodecRoutePolicy(overrides = mapOf(
            CodecFormat.PNG to CodecRoute.DEFAULT,
            CodecFormat.JPEG to CodecRoute.OS_FIRST,
            CodecFormat.GIF to CodecRoute.INTERNAL_ONLY,
            CodecFormat.WEBP to CodecRoute.OS_ONLY,
        ))
        assertThat(policy.orderFor(CodecFormat.PNG)).containsExactly(CodecBackend.INTERNAL, CodecBackend.OS).inOrder()
        assertThat(policy.orderFor(CodecFormat.JPEG)).containsExactly(CodecBackend.OS, CodecBackend.INTERNAL).inOrder()
        assertThat(policy.orderFor(CodecFormat.GIF)).containsExactly(CodecBackend.INTERNAL)
        assertThat(policy.orderFor(CodecFormat.WEBP)).containsExactly(CodecBackend.OS)
    }

    @Test
    fun preferredBackendFailuresFallBackBothWays() = runTest {
        assertThat(CodecRouteExecutor.execute(
            listOf(CodecBackend.INTERNAL, CodecBackend.OS),
            internal = { throw UnsupportedCodecException("internal failed") },
            os = { "os" },
        )).isEqualTo("os")
        assertThat(CodecRouteExecutor.execute(
            listOf(CodecBackend.OS, CodecBackend.INTERNAL),
            internal = { "internal" },
            os = { throw UnsupportedCodecException("os failed") },
        )).isEqualTo("internal")
    }

    @Test
    fun onlyRouteNeverCallsOtherBackend() {
        var osCalled = false
        val error = assertThrows(CodecRouteException::class.java) {
            runTest {
                CodecRouteExecutor.execute<String>(
                    listOf(CodecBackend.INTERNAL),
                    internal = { throw UnsupportedCodecException("failed") },
                    os = { osCalled = true; "unexpected" },
                )
            }
        }
        assertThat(error.failures).hasSize(1)
        assertThat(osCalled).isFalse()
    }

    @Test
    fun directBufferLimitCheckDoesNotAdvanceCallerPosition() {
        val buffer = ByteBuffer.allocateDirect(8).apply { position(2); limit(6) }.asReadOnlyBuffer()
        assertThrows(IllegalArgumentException::class.java) {
            runTest {
                AndroidCodecRouter().decode(
                    buffer,
                    "image/png",
                    DecodeOptions(maxEncodedBytes = 3),
                )
            }
        }
        assertThat(buffer.position()).isEqualTo(2)
        assertThat(buffer.limit()).isEqualTo(6)
    }

    @Test
    fun availableEncodersRespectTheEffectiveRoute() {
        val os = setOf(OsEncodeFormat.PNG, OsEncodeFormat.WEBP_LOSSY)
        val internal = setOf(OsEncodeFormat.PNG, OsEncodeFormat.WEBP_LOSSLESS)

        assertThat(
            availableEncodeFormatsForPolicy(
                CodecRoutePolicy(global = CodecRoute.OS_ONLY),
                os,
                internal,
            ),
        ).containsExactly(OsEncodeFormat.PNG, OsEncodeFormat.WEBP_LOSSY)
        assertThat(
            availableEncodeFormatsForPolicy(
                CodecRoutePolicy(global = CodecRoute.INTERNAL_ONLY),
                os,
                internal,
            ),
        ).containsExactly(OsEncodeFormat.PNG, OsEncodeFormat.WEBP_LOSSLESS)
    }

    @Test
    fun perFormatOverrideCanRemoveAnInternalOnlyEncoder() {
        val result = availableEncodeFormatsForPolicy(
            policy = CodecRoutePolicy(
                global = CodecRoute.INTERNAL_FIRST,
                overrides = mapOf(CodecFormat.WEBP to CodecRoute.OS_ONLY),
            ),
            osFormats = setOf(OsEncodeFormat.PNG),
            internalFormats = setOf(OsEncodeFormat.PNG, OsEncodeFormat.WEBP_LOSSLESS),
        )

        assertThat(result).containsExactly(OsEncodeFormat.PNG)
    }
}
