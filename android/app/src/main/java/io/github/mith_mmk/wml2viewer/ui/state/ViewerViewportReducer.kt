package io.github.mith_mmk.wml2viewer.ui.state

import androidx.compose.runtime.Immutable

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

    private fun scale(viewport: ViewerViewport, factor: Float): ViewerViewport {
        if (!factor.isFinite() || factor <= 0f) return viewport
        val currentZoom = viewport.zoom.takeIf { it.isFinite() }
            ?.coerceIn(MIN_ZOOM, MAX_ZOOM)
            ?: MIN_ZOOM
        val zoom = (currentZoom * factor).coerceIn(MIN_ZOOM, MAX_ZOOM)
        return if (zoom == MIN_ZOOM) reset() else viewport.copy(zoom = zoom)
    }
}
