package io.github.mith_mmk.wml2viewer

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.FilmstripItemUi
import io.github.mith_mmk.wml2viewer.ui.model.LanguagePreference
import io.github.mith_mmk.wml2viewer.ui.model.MangaPageRef
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.state.EmptyMobileViewerController
import io.github.mith_mmk.wml2viewer.ui.state.InMemoryMobileSettingsStore
import io.github.mith_mmk.wml2viewer.ui.state.ViewerEngineSnapshot
import io.github.mith_mmk.wml2viewer.ui.state.ViewerUiEvent
import io.github.mith_mmk.wml2viewer.ui.state.ViewerViewModel
import io.github.mith_mmk.wml2viewer.ui.state.ViewerViewModelFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@LargeTest
class AndroidActivityRecreationInstrumentationTest {
    @Test
    fun componentActivityHostSurvivesConfigurationRecreation() {
        ActivityScenario.launch(Wml2ViewerActivity::class.java).use { scenario ->
            lateinit var original: Wml2ViewerActivity
            scenario.onActivity { activity ->
                original = activity
                assertFalse(activity.isFinishing)
                assertFalse(activity.isDestroyed)
                assertTrue(activity.window.decorView.isAttachedToWindow)
            }
            scenario.recreate()
            scenario.onActivity { recreated ->
                assertNotSame(original, recreated)
                assertFalse(recreated.isFinishing)
                assertFalse(recreated.isDestroyed)
                assertTrue(recreated.window.decorView.isAttachedToWindow)
            }
        }
    }

    @Test
    fun rebuildingViewModelFromRestoredOwnersRestoresLogicalSelectionAndSettings() {
        val controller = EmptyMobileViewerController().apply {
            snapshot.value = ViewerEngineSnapshot(
                title = "Recreated page",
                mangaPages = listOf(
                    MangaPageRef("page-1", "book", portrait = true),
                    MangaPageRef("page-2", "book", portrait = true),
                ),
                currentLogicalPageIndex = 1,
                filmstrip = listOf(
                    FilmstripItemUi("page-1", "1", selected = false),
                    FilmstripItemUi("page-2", "2", selected = true),
                ),
            )
        }
        val settingsStore = InMemoryMobileSettingsStore(
            MobileViewerSettings(
                language = LanguagePreference.JAPANESE,
            ),
        )
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.runOnMainSync {
            val firstOwner = TestViewModelOwner()
            val first = ViewModelProvider(
                firstOwner,
                ViewerViewModelFactory(controller, settingsStore),
            )[ViewerViewModel::class.java]
            first.onEvent(ViewerUiEvent.WindowMetricsChanged(widthDp = 720f, isLandscape = true))
            assertEquals(DeviceClass.EXPANDED, first.uiState.value.deviceClass)
            assertTrue(first.uiState.value.isLandscape)
            assertEquals(1, first.uiState.value.engine.currentLogicalPageIndex)
            firstOwner.viewModelStore.clear()

            // UI/ViewModel objects are new while their owners expose restored snapshots. The
            // separate two-phase instrumentation test covers an actual process termination.
            val secondOwner = TestViewModelOwner()
            val recreated = ViewModelProvider(
                secondOwner,
                ViewerViewModelFactory(controller, settingsStore),
            )[ViewerViewModel::class.java]
            assertEquals(1, recreated.uiState.value.engine.currentLogicalPageIndex)
            assertEquals("page-2", recreated.uiState.value.engine.filmstrip.single { it.selected }.id)
            assertEquals(LanguagePreference.JAPANESE, recreated.uiState.value.settings.language)
            secondOwner.viewModelStore.clear()
        }
    }

    private class TestViewModelOwner : ViewModelStoreOwner {
        override val viewModelStore = ViewModelStore()
    }
}
