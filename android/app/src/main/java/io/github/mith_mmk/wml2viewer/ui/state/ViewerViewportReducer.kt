package io.github.mith_mmk.wml2viewer.ui.state

import androidx.compose.runtime.Immutable
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.touch.TapZoneResolver

@Immutable
internal data class ViewerViewport(
    val zoom: Float = ViewerViewportReducer.MIN_ZOOM,
    val panX: Float = 0f,
    val panY: Float = 0f,
)

/** Shared, side-effect-free viewport rules for gestures and configured viewer actions. */
internal object ViewerViewportReducer {
    const val MIN_ZOOM = 1f
    const val MAX_ZOOM = 8f
    const val ACTION_ZOOM_FACTOR = 1.25f

    fun zoomIn(viewport: ViewerViewport): ViewerViewport =
        scale(viewport, ACTION_ZOOM_FACTOR)

    fun zoomOut(viewport: ViewerViewport): ViewerViewport =
        scale(viewport, 1f / ACTION_ZOOM_FACTOR)

    fun transform(
        viewport: ViewerViewport,
        panX: Float,
        panY: Float,
        zoomChange: Float,
    ): ViewerViewport = scale(viewport, zoomChange).let { scaled ->
        if (scaled.zoom == MIN_ZOOM) scaled
        else scaled.copy(
            panX = scaled.panX + panX,
            panY = scaled.panY + panY,
        )
    }

    fun reset(): ViewerViewport = ViewerViewport()

    /** Keeps zoom across resize while bringing any no-longer-visible pan back into range. */
    fun clampToSurface(
        viewport: ViewerViewport,
        surfaceWidth: Int,
        surfaceHeight: Int,
        imageWidth: Int,
        imageHeight: Int,
        fit: DisplayFit,
    ): ViewerViewport {
        if (surfaceWidth <= 0 || surfaceHeight <= 0 || imageWidth <= 0 || imageHeight <= 0) {
            return viewport
        }
        val normalized = scale(viewport, 1f)
        if (normalized.zoom == MIN_ZOOM) return normalized
        val centered = TapZoneResolver.imageRect(
            surfaceWidth = surfaceWidth.toFloat(),
            surfaceHeight = surfaceHeight.toFloat(),
            imageWidth = imageWidth,
            imageHeight = imageHeight,
            fit = fit,
            zoom = normalized.zoom,
        ) ?: return normalized
        val maxPanX = ((centered.width - surfaceWidth) / 2f).coerceAtLeast(0f)
        val maxPanY = ((centered.height - surfaceHeight) / 2f).coerceAtLeast(0f)
        return normalized.copy(
            panX = normalized.panX.coerceIn(-maxPanX, maxPanX),
            panY = normalized.panY.coerceIn(-maxPanY, maxPanY),
        )
    }

    private fun scale(viewport: ViewerViewport, factor: Float): ViewerViewport {
        if (!factor.isFinite() || factor <= 0f) return viewport
        val currentZoom = viewport.zoom.takeIf { it.isFinite() }
            ?.coerceIn(MIN_ZOOM, MAX_ZOOM)
            ?: MIN_ZOOM
        val zoom = (currentZoom * factor).coerceIn(MIN_ZOOM, MAX_ZOOM)
        return if (zoom == MIN_ZOOM) reset() else viewport.copy(zoom = zoom)
    }
}
