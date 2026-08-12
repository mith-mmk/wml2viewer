package io.github.mith_mmk.wml2viewer.ui.touch

import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit

data class TouchInsets(
    val left: Float = 0f,
    val top: Float = 0f,
    val right: Float = 0f,
    val bottom: Float = 0f,
)

data class TouchRect(
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
) {
    val width: Float get() = right - left
    val height: Float get() = bottom - top

    fun intersect(other: TouchRect): TouchRect? {
        val intersection = TouchRect(
            left = maxOf(left, other.left),
            top = maxOf(top, other.top),
            right = minOf(right, other.right),
            bottom = minOf(bottom, other.bottom),
        )
        return intersection.takeIf { it.width > 0f && it.height > 0f }
    }
}

object TapZoneResolver {
    fun resolve(
        x: Float,
        y: Float,
        width: Float,
        height: Float,
        insets: TouchInsets = TouchInsets(),
    ): TapZone? {
        return resolve(
            x = x,
            y = y,
            rect = TouchRect(
                left = insets.left,
                top = insets.top,
                right = width - insets.right,
                bottom = height - insets.bottom,
            ),
        )
    }

    fun resolve(x: Float, y: Float, rect: TouchRect?): TapZone? {
        if (rect == null || rect.width <= 0f || rect.height <= 0f) return null
        if (x < rect.left || y < rect.top || x >= rect.right || y >= rect.bottom) return null

        val column = (((x - rect.left) / rect.width) * 3f).toInt().coerceIn(0, 2)
        val row = (((y - rect.top) / rect.height) * 3f).toInt().coerceIn(0, 2)
        return TapZone.at(row, column)
    }

    /** The app/viewer touch surface, independent of the displayed image bounds. */
    fun surfaceRect(
        surfaceWidth: Float,
        surfaceHeight: Float,
        insets: TouchInsets = TouchInsets(),
    ): TouchRect? {
        if (surfaceWidth <= 0f || surfaceHeight <= 0f) return null
        return TouchRect(
            left = insets.left.coerceAtLeast(0f),
            top = insets.top.coerceAtLeast(0f),
            right = surfaceWidth - insets.right.coerceAtLeast(0f),
            bottom = surfaceHeight - insets.bottom.coerceAtLeast(0f),
        ).takeIf { it.width > 0f && it.height > 0f }
    }

    /** Rectangle produced by ContentScale.Fit, centered inside the available surface. */
    fun fitImageRect(
        surfaceWidth: Float,
        surfaceHeight: Float,
        imageWidth: Int,
        imageHeight: Int,
    ): TouchRect? = imageRect(
        surfaceWidth = surfaceWidth,
        surfaceHeight = surfaceHeight,
        imageWidth = imageWidth,
        imageHeight = imageHeight,
        fit = DisplayFit.CONTAIN,
    )

    fun imageRect(
        surfaceWidth: Float,
        surfaceHeight: Float,
        imageWidth: Int,
        imageHeight: Int,
        fit: DisplayFit,
        zoom: Float = 1f,
        panX: Float = 0f,
        panY: Float = 0f,
    ): TouchRect? {
        if (surfaceWidth <= 0f || surfaceHeight <= 0f || imageWidth <= 0 || imageHeight <= 0) {
            return null
        }
        val scale = when (fit) {
            DisplayFit.CONTAIN -> minOf(
                surfaceWidth / imageWidth.toFloat(),
                surfaceHeight / imageHeight.toFloat(),
            )
            DisplayFit.WIDTH -> surfaceWidth / imageWidth.toFloat()
            DisplayFit.HEIGHT -> surfaceHeight / imageHeight.toFloat()
            DisplayFit.ORIGINAL -> 1f
        }
        val fittedWidth = imageWidth.toFloat() * scale
        val fittedHeight = imageHeight.toFloat() * scale
        val left = (surfaceWidth - fittedWidth) / 2f
        val top = (surfaceHeight - fittedHeight) / 2f
        val base = TouchRect(left, top, left + fittedWidth, top + fittedHeight)
        val safeZoom = zoom.coerceAtLeast(0.01f)
        val centerX = surfaceWidth / 2f
        val centerY = surfaceHeight / 2f
        return TouchRect(
            left = (base.left - centerX) * safeZoom + centerX + panX,
            top = (base.top - centerY) * safeZoom + centerY + panY,
            right = (base.right - centerX) * safeZoom + centerX + panX,
            bottom = (base.bottom - centerY) * safeZoom + centerY + panY,
        )
    }

    /** Visible portion of a transformed image; zone cells must never include clipped pixels. */
    fun visibleImageRect(
        imageRect: TouchRect?,
        surfaceWidth: Float,
        surfaceHeight: Float,
        insets: TouchInsets = TouchInsets(),
    ): TouchRect? {
        if (imageRect == null || surfaceWidth <= 0f || surfaceHeight <= 0f) return null
        return imageRect.intersect(
            TouchRect(
                left = insets.left.coerceAtLeast(0f),
                top = insets.top.coerceAtLeast(0f),
                right = (surfaceWidth - insets.right.coerceAtLeast(0f)),
                bottom = (surfaceHeight - insets.bottom.coerceAtLeast(0f)),
            ),
        )
    }
}
