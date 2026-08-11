package io.github.mith_mmk.wml2viewer.ui

import android.content.res.Configuration
import android.graphics.Bitmap
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.test.click
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.FilmstripItemUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.MobileScreen
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import io.github.mith_mmk.wml2viewer.ui.state.MobileViewerUiState
import io.github.mith_mmk.wml2viewer.ui.state.ViewerEngineSnapshot
import io.github.mith_mmk.wml2viewer.ui.state.ViewerUiEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class MobileLayoutInstrumentationTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun widthBelow600DpUsesCompactViewerEvenWhenPersistedClassIsExpanded() {
        val events = mutableListOf<ViewerUiEvent>()
        compose.setContent {
            MobileViewerContent(
                state = viewerState(deviceClass = DeviceClass.EXPANDED),
                onEvent = events::add,
                modifier = Modifier.requiredSize(width = 599.dp, height = 720.dp),
            )
        }

        compose.onNodeWithTag("viewer-pane").fetchSemanticsNode()
        assertTrue(compose.onAllNodesWithTag("expanded-two-pane").fetchSemanticsNodes().isEmpty())
        compose.runOnIdle {
            val metric = events.filterIsInstance<ViewerUiEvent.WindowMetricsChanged>().last()
            assertTrue(metric.widthDp < 600f)
        }
    }

    @Test
    fun widthAtLeast600DpUsesFixedFilerAndViewerPanes() {
        val events = mutableListOf<ViewerUiEvent>()
        compose.setContent {
            TabletConfiguration {
                MobileViewerContent(
                    state = viewerState(deviceClass = DeviceClass.COMPACT),
                    onEvent = events::add,
                    modifier = Modifier.requiredSize(width = 700.dp, height = 600.dp),
                )
            }
        }

        compose.onNodeWithTag("expanded-two-pane").fetchSemanticsNode()
        compose.onNodeWithTag("filer-pane").fetchSemanticsNode()
        compose.onNodeWithTag("viewer-pane").fetchSemanticsNode()
        compose.runOnIdle {
            val metric = events.filterIsInstance<ViewerUiEvent.WindowMetricsChanged>().last()
            assertTrue(metric.widthDp >= 600f)
        }
    }

    @Test
    fun expandedFilerSeparatesFolderNavigatorFromFileListAt840Dp() {
        val entries = listOf(
            FilerEntryUi("folder", "Folder", isContainer = true),
            FilerEntryUi("file", "Page.jpg", isContainer = false),
        )
        compose.setContent {
            TabletConfiguration {
                MobileViewerContent(
                    state = viewerState(
                        engine = ViewerEngineSnapshot(filerEntries = entries),
                    ).copy(screen = MobileScreen.FILER),
                    onEvent = {},
                    modifier = Modifier.requiredSize(width = 840.dp, height = 600.dp),
                )
            }
        }

        val navigator = compose.onNodeWithTag("filer-pane").fetchSemanticsNode().boundsInRoot
        val fileList = compose.onNodeWithTag("tablet-list-pane").fetchSemanticsNode().boundsInRoot
        compose.onNodeWithTag("filer-entry-folder").assertIsDisplayed()
        compose.onNodeWithTag("filer-entry-file").assertIsDisplayed()
        assertTrue(navigator.right <= fileList.left)
        assertTrue(navigator.width < fileList.width)
    }

    @Test
    fun lightThemeRequestsDarkSystemBarIcons() {
        var darkBars: Boolean? = null
        val settings = MobileViewerSettings().copy(theme = ThemeMode.LIGHT)
        compose.setContent {
            MobileViewerContent(
                state = viewerState().copy(settings = settings),
                onEvent = {},
                hostCallbacks = MobileUiHostCallbacks(
                    applyDarkSystemBars = { darkBars = it },
                ),
            )
        }

        compose.runOnIdle { assertEquals(false, darkBars) }
    }

    @Test
    fun orientationRecompositionKeepsLogicalPageAndSelectedPosition() {
        val landscape = mutableStateOf(false)
        val events = mutableListOf<ViewerUiEvent>()
        val state = viewerState(
            engine = ViewerEngineSnapshot(
                title = "Logical page 8",
                currentLogicalPageIndex = 7,
                filmstrip = listOf(FilmstripItemUi("page-8", "8", selected = true)),
            ),
        )
        compose.setContent {
            val baseConfiguration = LocalConfiguration.current
            val orientationConfiguration = Configuration(baseConfiguration).apply {
                orientation = if (landscape.value) {
                    Configuration.ORIENTATION_LANDSCAPE
                } else {
                    Configuration.ORIENTATION_PORTRAIT
                }
            }
            CompositionLocalProvider(LocalConfiguration provides orientationConfiguration) {
                MobileViewerContent(
                    state = state,
                    onEvent = events::add,
                    modifier = Modifier.requiredSize(width = 599.dp, height = 420.dp),
                )
            }
        }

        compose.onNodeWithText("Logical page 8").fetchSemanticsNode()
        compose.runOnIdle { landscape.value = true }
        compose.waitForIdle()
        compose.onNodeWithText("Logical page 8").fetchSemanticsNode()
        compose.runOnIdle {
            assertEquals(7, state.engine.currentLogicalPageIndex)
            assertTrue(state.engine.filmstrip.single().selected)
            assertTrue(events.filterIsInstance<ViewerUiEvent.WindowMetricsChanged>().last().isLandscape)
        }
    }

    @Test
    fun widthClassChangeKeepsFilerScrollPosition() {
        val width = mutableStateOf(599.dp)
        val entries = (0 until 40).map { index ->
            FilerEntryUi("entry-$index", "Page $index", isContainer = false)
        }
        compose.setContent {
            TabletConfiguration {
                MobileViewerContent(
                    state = viewerState(
                        engine = ViewerEngineSnapshot(filerEntries = entries),
                    ).copy(screen = MobileScreen.FILER),
                    onEvent = {},
                    modifier = Modifier.requiredSize(width = width.value, height = 360.dp),
                )
            }
        }

        compose.onNodeWithTag("filer-pane-list")
            .performScrollToNode(hasTestTag("filer-entry-entry-30"))
        compose.onNodeWithTag("filer-entry-entry-30").assertIsDisplayed()

        compose.runOnIdle { width.value = 700.dp }
        compose.waitForIdle()
        compose.onNodeWithTag("filer-entry-entry-30").assertIsDisplayed()
    }

    @Test
    fun phoneLandscapeViewerHidesPersistentFilerAndTopChrome() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        compose.setContent {
            val phoneLandscape = Configuration(LocalConfiguration.current).apply {
                orientation = Configuration.ORIENTATION_LANDSCAPE
                smallestScreenWidthDp = 411
            }
            CompositionLocalProvider(LocalConfiguration provides phoneLandscape) {
                MobileViewerContent(
                    state = viewerState(deviceClass = DeviceClass.EXPANDED).copy(isLandscape = true),
                    onEvent = {},
                    modifier = Modifier.requiredSize(width = 700.dp, height = 360.dp),
                )
            }
        }

        compose.onNodeWithTag("viewer-pane").assertIsDisplayed()
        assertTrue(compose.onAllNodesWithTag("expanded-two-pane").fetchSemanticsNodes().isEmpty())
        assertTrue(compose.onAllNodesWithTag("filer-pane").fetchSemanticsNodes().isEmpty())
        assertTrue(
            compose.onAllNodesWithText(
                context.getString(R.string.viewer_open_settings),
            ).fetchSemanticsNodes().isEmpty(),
        )
    }

    @Test
    fun tabletLandscapeKeepsFixedFilerAndTopChrome() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        compose.setContent {
            TabletConfiguration(orientation = Configuration.ORIENTATION_LANDSCAPE) {
                MobileViewerContent(
                    state = viewerState(deviceClass = DeviceClass.EXPANDED).copy(isLandscape = true),
                    onEvent = {},
                    modifier = Modifier.requiredSize(width = 800.dp, height = 600.dp),
                )
            }
        }

        compose.onNodeWithTag("expanded-two-pane").assertIsDisplayed()
        compose.onNodeWithTag("filer-pane").assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.viewer_open_settings))
            .assertIsDisplayed()
    }

    @Test
    fun tapZonesUseDisplayedImageRectAndIgnoreLetterbox() {
        val bitmap = Bitmap.createBitmap(100, 300, Bitmap.Config.ARGB_8888)
        val events = mutableListOf<ViewerUiEvent>()
        compose.setContent {
            MobileViewerContent(
                state = viewerState(
                    engine = ViewerEngineSnapshot(frame = bitmap.asImageBitmap()),
                    showTopChrome = false,
                ),
                onEvent = events::add,
                modifier = Modifier.requiredSize(360.dp),
            )
        }
        val surface = compose.onNodeWithTag("viewer-surface")
        val size = surface.fetchSemanticsNode().size

        surface.performTouchInput {
            click(Offset(size.width * 0.05f, size.height * 0.10f))
        }
        compose.runOnIdle {
            assertFalse(events.any { it is ViewerUiEvent.TapZonePressed })
        }

        compose.mainClock.advanceTimeBy(1_000L)
        surface.performTouchInput {
            click(Offset(size.width * 0.65f, size.height * 0.10f))
        }
        // detectTapGestures defers a single tap while waiting for a possible
        // double tap, so advance past that decision window before asserting.
        compose.mainClock.advanceTimeBy(1_000L)
        compose.waitForIdle()
        compose.runOnIdle {
            val taps = events.filterIsInstance<ViewerUiEvent.TapZonePressed>()
            assertEquals(TapZone.TOP_RIGHT, taps.single().zone)
        }
    }

    @Test
    fun filmstripPanelConsumesBlankAreaWithoutTriggeringViewerZone() {
        val bitmap = Bitmap.createBitmap(300, 300, Bitmap.Config.ARGB_8888)
        val events = mutableListOf<ViewerUiEvent>()
        compose.setContent {
            MobileViewerContent(
                state = viewerState(
                    engine = ViewerEngineSnapshot(frame = bitmap.asImageBitmap()),
                    showTopChrome = false,
                ).copy(subfilerVisible = true),
                onEvent = events::add,
                modifier = Modifier.requiredSize(420.dp),
            )
        }

        compose.onNodeWithTag("filmstrip").performTouchInput { click() }
        compose.runOnIdle {
            assertFalse(events.any { it is ViewerUiEvent.TapZonePressed })
        }
    }

    @Composable
    private fun TabletConfiguration(
        orientation: Int = Configuration.ORIENTATION_PORTRAIT,
        content: @Composable () -> Unit,
    ) {
        val tablet = Configuration(LocalConfiguration.current).apply {
            this.orientation = orientation
            smallestScreenWidthDp = 700
        }
        CompositionLocalProvider(LocalConfiguration provides tablet, content = content)
    }

    private fun viewerState(
        deviceClass: DeviceClass = DeviceClass.COMPACT,
        engine: ViewerEngineSnapshot = ViewerEngineSnapshot(title = "Instrumentation page"),
        showTopChrome: Boolean = true,
    ): MobileViewerUiState = MobileViewerUiState(
        deviceClass = deviceClass,
        screen = MobileScreen.VIEWER,
        engine = engine,
        settings = MobileViewerSettings().let { settings ->
            settings.copy(viewing = settings.viewing.copy(showTopChrome = showTopChrome))
        },
    )
}
