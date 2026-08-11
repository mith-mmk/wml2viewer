package io.github.mith_mmk.wml2viewer

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.assertWidthIsAtLeast
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.ui.components.TouchMapEditor
import io.github.mith_mmk.wml2viewer.ui.components.consumeViewerInput
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.TouchMapConfig
import io.github.mith_mmk.wml2viewer.ui.theme.CinematicDarkTheme
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue

class TouchMapEditorTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun exposesAllNineConfigurableCells() {
        compose.setContent {
            var config by remember { mutableStateOf(TouchMapConfig()) }
            CinematicDarkTheme {
                TouchMapEditor(config = config, onChange = { config = it })
            }
        }

        TapZone.entries.forEach { zone ->
            compose.onNodeWithTag("touch-zone-${zone.name.lowercase()}")
                .assertHeightIsAtLeast(48.dp)
                .assertWidthIsAtLeast(48.dp)
        }
        compose.onNodeWithTag("touch-map-reset")
            .assertHeightIsAtLeast(48.dp)
            .assertWidthIsAtLeast(48.dp)
    }

    @Test
    fun rtlLocaleKeepsPhysicalLeftAndRightZoneEditorsInTheirScreenColumns() {
        compose.setContent {
            CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) {
                CinematicDarkTheme {
                    TouchMapEditor(config = TouchMapConfig(), onChange = {})
                }
            }
        }

        val physicalLeft = compose.onNodeWithTag("touch-zone-top_left")
            .fetchSemanticsNode().boundsInRoot
        val physicalRight = compose.onNodeWithTag("touch-zone-top_right")
            .fetchSemanticsNode().boundsInRoot
        assertTrue(physicalLeft.left < physicalRight.left)
    }

    @Test
    fun panelBlankAreaDoesNotFallThroughToViewer() {
        var viewerTaps = 0
        compose.setContent {
            Box(Modifier.size(200.dp)) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .pointerInput(Unit) {
                            detectTapGestures { viewerTaps += 1 }
                        },
                )
                Surface(
                    modifier = Modifier
                        .fillMaxSize()
                        .consumeViewerInput()
                        .testTag("blocking-panel"),
                ) {}
            }
        }

        compose.onNodeWithTag("blocking-panel").performTouchInput { click() }
        compose.runOnIdle { assertEquals(0, viewerTaps) }
    }
}
