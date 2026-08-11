package io.github.mith_mmk.wml2viewer.platform.codec

import com.google.common.truth.Truth.assertThat
import java.nio.ByteBuffer
import org.junit.Test

class NativeRustCodecFallbackTest {
    @Test
    fun argbRowIsConvertedToRgbaWithoutNativeLoading() {
        val output = ByteBuffer.allocateDirect(8)

        ArgbToRgba.appendRow(
            output,
            intArrayOf(0x7F123456, 0xFFEEDDCC.toInt()),
        )

        output.flip()
        val actual = ByteArray(output.remaining()).also(output::get)
        assertThat(actual.map { it.toInt() and 0xFF }).containsExactly(
            0x12,
            0x34,
            0x56,
            0x7F,
            0xEE,
            0xDD,
            0xCC,
            0xFF,
        ).inOrder()
    }
}
