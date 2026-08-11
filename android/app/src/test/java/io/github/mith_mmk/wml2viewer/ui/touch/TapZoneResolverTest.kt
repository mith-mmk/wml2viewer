package io.github.mith_mmk.wml2viewer.ui.touch

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.TouchMapConfig
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import org.junit.Test

class TapZoneResolverTest {
    @Test
    fun resolvesAllNineCells() {
        TapZone.entries.forEach { zone ->
            val x = zone.column * 100f + 50f
            val y = zone.row * 100f + 50f
            assertThat(TapZoneResolver.resolve(x, y, 300f, 300f)).isEqualTo(zone)
        }
    }

    @Test
    fun contentScaleFitExcludesLetterbox() {
        val rect = TapZoneResolver.fitImageRect(
            surfaceWidth = 400f,
            surfaceHeight = 300f,
            imageWidth = 100,
            imageHeight = 100,
        )

        assertThat(rect).isEqualTo(TouchRect(50f, 0f, 350f, 300f))
        assertThat(TapZoneResolver.resolve(25f, 150f, rect)).isNull()
        assertThat(TapZoneResolver.resolve(375f, 150f, rect)).isNull()
        assertThat(TapZoneResolver.resolve(100f, 50f, rect)).isEqualTo(TapZone.TOP_LEFT)
        assertThat(TapZoneResolver.resolve(200f, 150f, rect)).isEqualTo(TapZone.CENTER)
        assertThat(TapZoneResolver.resolve(300f, 250f, rect)).isEqualTo(TapZone.BOTTOM_RIGHT)
    }

    @Test
    fun invalidGeometryAndExclusiveRightBottomReturnNull() {
        assertThat(TapZoneResolver.fitImageRect(0f, 100f, 10, 10)).isNull()
        val rect = TouchRect(10f, 20f, 310f, 320f)
        assertThat(TapZoneResolver.resolve(310f, 100f, rect)).isNull()
        assertThat(TapZoneResolver.resolve(100f, 320f, rect)).isNull()
    }

    @Test
    fun displayFitModesAndTransformShareTheHitRectangleMath() {
        val width = TapZoneResolver.imageRect(400f, 300f, 100, 200, DisplayFit.WIDTH)
        val height = TapZoneResolver.imageRect(400f, 300f, 100, 200, DisplayFit.HEIGHT)
        val original = TapZoneResolver.imageRect(400f, 300f, 100, 200, DisplayFit.ORIGINAL)
        val transformed = TapZoneResolver.imageRect(
            400f,
            300f,
            100,
            200,
            DisplayFit.ORIGINAL,
            zoom = 2f,
            panX = 10f,
            panY = -20f,
        )

        assertThat(width).isEqualTo(TouchRect(0f, -250f, 400f, 550f))
        assertThat(height).isEqualTo(TouchRect(125f, 0f, 275f, 300f))
        assertThat(original).isEqualTo(TouchRect(150f, 50f, 250f, 250f))
        assertThat(transformed).isEqualTo(TouchRect(110f, -70f, 310f, 330f))
    }

    @Test
    fun defaultTouchMapMatchesMobilePreset() {
        val map = TouchMapConfig()
        TapZone.entries.filter { it.column == 0 }.forEach {
            assertThat(map.actionFor(it)).isEqualTo(ViewerAction.PREVIOUS_IMAGE)
        }
        TapZone.entries.filter { it.column == 2 }.forEach {
            assertThat(map.actionFor(it)).isEqualTo(ViewerAction.NEXT_IMAGE)
        }
        assertThat(map.actionFor(TapZone.TOP_CENTER)).isEqualTo(ViewerAction.OPEN_FILER)
        assertThat(map.actionFor(TapZone.CENTER)).isEqualTo(ViewerAction.OPEN_SETTINGS)
        assertThat(map.actionFor(TapZone.BOTTOM_CENTER)).isEqualTo(ViewerAction.OPEN_SUBFILER)
    }
}
