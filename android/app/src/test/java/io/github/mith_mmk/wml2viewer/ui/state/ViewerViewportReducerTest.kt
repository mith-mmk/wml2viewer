package io.github.mith_mmk.wml2viewer.ui.state

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class ViewerViewportReducerTest {
    @Test
    fun actionZoomUsesStableStepAndSharedBounds() {
        val zoomed = ViewerViewportReducer.zoomIn(ViewerViewport())
        assertThat(zoomed.zoom).isWithin(0.0001f).of(1.25f)

        val maximum = ViewerViewportReducer.zoomIn(
            ViewerViewport(zoom = 7.9f, panX = 12f, panY = -4f),
        )
        assertThat(maximum.zoom).isEqualTo(8f)
        assertThat(maximum.panX).isEqualTo(12f)
        assertThat(maximum.panY).isEqualTo(-4f)
    }

    @Test
    fun zoomOutAtMinimumResetsPanAndInvalidFactorDoesNotCorruptState() {
        val minimum = ViewerViewportReducer.zoomOut(
            ViewerViewport(zoom = 1.1f, panX = 20f, panY = -10f),
        )
        assertThat(minimum).isEqualTo(ViewerViewport())

        val current = ViewerViewport(zoom = 2f, panX = 3f, panY = 4f)
        assertThat(
            ViewerViewportReducer.transform(current, 10f, 20f, Float.NaN),
        ).isEqualTo(current.copy(panX = 13f, panY = 24f))
    }

    @Test
    fun gestureAndActionZoomShareTheSameMaximum() {
        val gesture = ViewerViewportReducer.transform(
            viewport = ViewerViewport(zoom = 4f),
            panX = 0f,
            panY = 0f,
            zoomChange = 3f,
        )
        assertThat(gesture.zoom).isEqualTo(ViewerViewportReducer.MAX_ZOOM)
    }
}
