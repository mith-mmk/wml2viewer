package io.github.mith_mmk.wml2viewer.data.controller

import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.ImageBitmapConfig
import androidx.compose.ui.graphics.colorspace.ColorSpace
import androidx.compose.ui.graphics.colorspace.ColorSpaces

/** Pure JVM fixture which avoids Robolectric Bitmap allocation in policy-only tests. */
internal class TestImageBitmap(
    override val width: Int,
    override val height: Int,
) : ImageBitmap {
    override val colorSpace: ColorSpace = ColorSpaces.Srgb
    override val hasAlpha: Boolean = true
    override val config: ImageBitmapConfig = ImageBitmapConfig.Argb8888

    override fun readPixels(
        buffer: IntArray,
        startX: Int,
        startY: Int,
        width: Int,
        height: Int,
        bufferOffset: Int,
        stride: Int,
    ) {
        require(startX >= 0 && startY >= 0 && width >= 0 && height >= 0)
        require(startX + width <= this.width && startY + height <= this.height)
        require(bufferOffset >= 0 && stride >= width)
        if (width == 0 || height == 0) return
        require(bufferOffset.toLong() + (height - 1L) * stride + width <= buffer.size)
        repeat(height) { row ->
            buffer.fill(0, bufferOffset + row * stride, bufferOffset + row * stride + width)
        }
    }

    override fun prepareToDraw() = Unit
}
