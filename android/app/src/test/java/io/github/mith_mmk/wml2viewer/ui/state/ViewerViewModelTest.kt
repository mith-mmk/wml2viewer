package io.github.mith_mmk.wml2viewer.ui.state

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.MangaPageRef
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.FilerCapabilitiesUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationType
import io.github.mith_mmk.wml2viewer.ui.model.MangaLayoutMode
import io.github.mith_mmk.wml2viewer.ui.model.MobileScreen
import io.github.mith_mmk.wml2viewer.ui.model.PendingTransferUi
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun rotationAndLogicalIndexChangeRequestCurrentSpreadWithoutFrameLoop() = runTest(dispatcher) {
        val pages = (0..3).map { MangaPageRef(it.toString(), "source", portrait = true) }
        val controller = RecordingController(
            ViewerEngineSnapshot(mangaPages = pages, currentLogicalPageIndex = 1),
        )
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())

        viewModel.onEvent(ViewerUiEvent.WindowMetricsChanged(800f, isLandscape = true))
        advanceUntilIdle()
        assertThat(controller.requests.last().currentLogicalPageIndex).isEqualTo(1)
        assertThat(controller.requests.last().landscape).isTrue()

        controller.snapshot.value = controller.snapshot.value.copy(currentLogicalPageIndex = 3)
        advanceUntilIdle()
        assertThat(controller.requests.last().currentLogicalPageIndex).isEqualTo(3)
        val requestCount = controller.requests.size

        controller.snapshot.value = controller.snapshot.value.copy(spreadFrames = emptyList())
        advanceUntilIdle()
        assertThat(controller.requests).hasSize(requestCount)
    }

    @Test
    fun viewportResizeSuppressesTouchUntilMatchingSpreadGenerationArrives() = runTest(dispatcher) {
        val pages = listOf(MangaPageRef("0", "source", portrait = true))
        val controller = RecordingController(
            ViewerEngineSnapshot(mangaPages = pages, currentLogicalPageIndex = 0),
        )
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        advanceUntilIdle()

        viewModel.onEvent(ViewerUiEvent.ViewportSizeChanged(800, 400))
        runCurrent()

        val generation = viewModel.uiState.value.viewportGeneration
        assertThat(generation).isGreaterThan(0L)
        assertThat(viewModel.uiState.value.touchReady).isFalse()
        assertThat(controller.requests.last().viewportGeneration).isEqualTo(generation)

        controller.snapshot.value = controller.snapshot.value.copy(
            renderedViewportGeneration = generation,
        )
        advanceUntilIdle()

        assertThat(viewModel.uiState.value.touchReady).isTrue()
    }

    @Test
    fun viewportResizeWithoutMangaPagesKeepsTouchEnabled() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot(title = "Single frame"))
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        advanceUntilIdle()

        viewModel.onEvent(ViewerUiEvent.ViewportSizeChanged(600, 300))
        advanceUntilIdle()

        assertThat(viewModel.uiState.value.touchReady).isTrue()
        assertThat(viewModel.uiState.value.viewportGeneration).isEqualTo(0L)
        assertThat(controller.requests).isEmpty()
    }

    @Test
    fun externallyChangedMangaSettingsImmediatelyReplanCurrentLogicalPage() = runTest(dispatcher) {
        val pages = (0..3).map { MangaPageRef(it.toString(), "source", portrait = true) }
        val controller = RecordingController(
            ViewerEngineSnapshot(mangaPages = pages, currentLogicalPageIndex = 2),
        )
        val settingsStore = InMemoryMobileSettingsStore()
        ViewerViewModel(controller, settingsStore)
        advanceUntilIdle()

        settingsStore.replace(
            settingsStore.settings.value.copy(
                manga = settingsStore.settings.value.manga.copy(layoutMode = MangaLayoutMode.SINGLE),
            ),
        )
        advanceUntilIdle()

        assertThat(controller.requests).hasSize(1)
        assertThat(controller.requests.single().currentLogicalPageIndex).isEqualTo(2)
    }

    @Test
    fun fitToggleIsPageLocalAndDoesNotRewriteInitialFitSetting() = runTest(dispatcher) {
        val pages = listOf(
            MangaPageRef("0", "source", portrait = true),
            MangaPageRef("1", "source", portrait = true),
        )
        val controller = RecordingController(
            ViewerEngineSnapshot(mangaPages = pages, currentLogicalPageIndex = 0),
        )
        val settingsStore = InMemoryMobileSettingsStore()
        val viewModel = ViewerViewModel(controller, settingsStore)

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.TOGGLE_FIT_MODE))
        advanceUntilIdle()
        assertThat(viewModel.uiState.value.fitOverride).isEqualTo(DisplayFit.ORIGINAL)
        assertThat(settingsStore.settings.value.viewing.fit).isEqualTo(DisplayFit.CONTAIN)

        controller.snapshot.value = controller.snapshot.value.copy(currentLogicalPageIndex = 1)
        advanceUntilIdle()
        assertThat(viewModel.uiState.value.fitOverride).isNull()
    }

    @Test
    fun compactFilerBackNavigatesUpBeforeLeavingFiler() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot(atSourceRoot = false))
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        viewModel.onEvent(ViewerUiEvent.WindowMetricsChanged(400f, false))
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_FILER))
        viewModel.onEvent(ViewerUiEvent.Back)
        advanceUntilIdle()

        assertThat(controller.navigateUpCalls).isEqualTo(1)
        assertThat(viewModel.uiState.value.screen.name).isEqualTo("FILER")
    }

    @Test
    fun compactFilerCloseLeavesNestedFolderAndCancelsPendingTransfer() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot(atSourceRoot = false))
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        viewModel.onEvent(ViewerUiEvent.WindowMetricsChanged(700f, true, isTablet = false))
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_FILER))
        viewModel.onEvent(
            ViewerUiEvent.BeginFilerTransfer(
                PendingTransferUi(FilerOperationType.COPY, "entry", "Page.jpg"),
            ),
        )

        viewModel.onEvent(ViewerUiEvent.CloseFiler)
        advanceUntilIdle()

        assertThat(controller.navigateUpCalls).isEqualTo(0)
        assertThat(viewModel.uiState.value.screen).isEqualTo(MobileScreen.VIEWER)
        assertThat(viewModel.uiState.value.pendingTransfer).isNull()
    }

    @Test
    fun phoneRemainsCompactWhenItsLandscapeWidthExceeds600Dp() = runTest(dispatcher) {
        val viewModel = ViewerViewModel(
            RecordingController(ViewerEngineSnapshot()),
            InMemoryMobileSettingsStore(),
        )

        viewModel.onEvent(
            ViewerUiEvent.WindowMetricsChanged(
                widthDp = 700f,
                isLandscape = true,
                isTablet = false,
            ),
        )
        advanceUntilIdle()

        assertThat(viewModel.uiState.value.deviceClass).isEqualTo(DeviceClass.COMPACT)
        assertThat(viewModel.uiState.value.isLandscape).isTrue()
    }

    @Test
    fun selectingFileShowsViewerBeforeSlowDecodeCompletes() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot())
        controller.selectionGate = CompletableDeferred()
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())

        viewModel.onEvent(ViewerUiEvent.WindowMetricsChanged(400f, false))
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_FILER))
        runCurrent()
        assertThat(viewModel.uiState.value.screen).isEqualTo(MobileScreen.FILER)

        viewModel.onEvent(ViewerUiEvent.SelectFilerEntry("file", isContainer = false))
        runCurrent()
        assertThat(viewModel.uiState.value.screen).isEqualTo(MobileScreen.VIEWER)

        controller.selectionGate?.complete(Unit)
        advanceUntilIdle()
    }

    @Test
    fun smbEventSecretIsClearedSynchronously() = runTest(dispatcher) {
        val input = SmbConnectionInput(server = "nas", password = charArrayOf('x', 'y'))
        val controller = RecordingController(ViewerEngineSnapshot())
        val viewModel = ViewerViewModel(
            controller,
            InMemoryMobileSettingsStore(),
        )

        viewModel.onEvent(ViewerUiEvent.RequestSmbShares(input))

        assertThat(input.password).asList().containsExactly('\u0000', '\u0000')
        advanceUntilIdle()
        assertThat(controller.lastSmbInput?.password?.all { it == '\u0000' }).isTrue()
    }

    @Test
    fun savedSmbCredentialEventClearsEveryOwnedCopy() = runTest(dispatcher) {
        val input = SmbCredentialInput("saved-source", charArrayOf('n', 'e', 'w'))
        val controller = RecordingController(ViewerEngineSnapshot())
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())

        viewModel.onEvent(ViewerUiEvent.ReenterSmbCredential(input))

        assertThat(input.password).asList().containsExactly('\u0000', '\u0000', '\u0000')
        advanceUntilIdle()
        assertThat(controller.lastCredentialInput?.password?.all { it == '\u0000' }).isTrue()
    }

    @Test
    fun longPressQuickMenuIsFunctionalAndBackDismissesIt() = runTest(dispatcher) {
        val viewModel = ViewerViewModel(
            RecordingController(ViewerEngineSnapshot()),
            InMemoryMobileSettingsStore(),
        )

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_CONTEXT_MENU))
        assertThat(viewModel.uiState.value.quickMenuVisible).isTrue()

        viewModel.onEvent(ViewerUiEvent.Back)
        assertThat(viewModel.uiState.value.quickMenuVisible).isFalse()
    }

    @Test
    fun zoomAndGrayscaleActionsUpdateViewerStateWithoutControllerDispatch() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot())
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.ZOOM_IN))
        assertThat(viewModel.uiState.value.zoom).isWithin(0.0001f).of(1.25f)

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.ZOOM_OUT))
        assertThat(viewModel.uiState.value.zoom).isEqualTo(1f)

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.TOGGLE_GRAYSCALE))
        assertThat(viewModel.uiState.value.grayscaleEnabled).isTrue()
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.TOGGLE_GRAYSCALE))
        assertThat(viewModel.uiState.value.grayscaleEnabled).isFalse()

        advanceUntilIdle()
        assertThat(controller.dispatchedActions).isEmpty()
    }

    @Test
    fun currentDirectoryExportCallsControllerDirectly() = runTest(dispatcher) {
        val controller = exportController()
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        val request = ExportRequest(
            ExportFormat.PNG,
            quality = 90,
            fileName = "page.png",
            destination = ExportDestination.CURRENT_DIRECTORY,
        )

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.EXPORT))
        assertThat(viewModel.uiState.value.exportDialogVisible).isTrue()
        viewModel.onEvent(ViewerUiEvent.SubmitExport(request))
        advanceUntilIdle()

        assertThat(viewModel.uiState.value.exportDialogVisible).isFalse()
        assertThat(controller.exports).containsExactly(request to null)
    }

    @Test
    fun systemExportBlocksDuplicatesAndCancelReleasesPendingRequest() = runTest(dispatcher) {
        val controller = exportController()
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        val request = ExportRequest(
            ExportFormat.JPEG,
            quality = 85,
            fileName = "page.jpg",
            destination = ExportDestination.SYSTEM_PICKER,
        )
        val effect = async { viewModel.effects.first() }

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.EXPORT))
        viewModel.onEvent(ViewerUiEvent.SubmitExport(request))
        advanceUntilIdle()
        assertThat(effect.await()).isEqualTo(ViewerUiEffect.CreateExportDocument(request))

        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.EXPORT))
        assertThat(viewModel.uiState.value.exportDialogVisible).isFalse()
        viewModel.onEvent(ViewerUiEvent.SubmitExport(request.copy(fileName = "duplicate.jpg")))
        advanceUntilIdle()
        assertThat(controller.exports).isEmpty()

        viewModel.onEvent(ViewerUiEvent.ExportDocumentCancelled)
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.EXPORT))
        assertThat(viewModel.uiState.value.exportDialogVisible).isTrue()
    }

    @Test
    fun systemExportResultPassesUriAndReleasesPendingRequest() = runTest(dispatcher) {
        val controller = exportController()
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        val request = ExportRequest(
            ExportFormat.WEBP_LOSSY,
            quality = 75,
            fileName = "page.webp",
            destination = ExportDestination.SYSTEM_PICKER,
        )

        viewModel.onEvent(ViewerUiEvent.SubmitExport(request))
        viewModel.effects.first()
        viewModel.onEvent(ViewerUiEvent.ExportDocumentCreated("content://export/result"))
        advanceUntilIdle()

        assertThat(controller.exports).containsExactly(request to "content://export/result")
        viewModel.onEvent(ViewerUiEvent.PerformAction(ViewerAction.EXPORT))
        assertThat(viewModel.uiState.value.exportDialogVisible).isTrue()
    }

    @Test
    fun restoredSystemExportResultSurvivesProcessRecreation() = runTest(dispatcher) {
        val controller = exportController()
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())
        val request = ExportRequest(
            ExportFormat.PNG,
            quality = 90,
            fileName = "restored.png",
            destination = ExportDestination.SYSTEM_PICKER,
        )

        viewModel.onEvent(
            ViewerUiEvent.ExportDocumentCreated(
                uriToken = "content://export/restored",
                restoredRequest = request,
            ),
        )
        advanceUntilIdle()

        assertThat(controller.exports).containsExactly(request to "content://export/restored")
    }

    @Test
    fun safPickerModesArePassedToTheControllerOwnedTransaction() = runTest(dispatcher) {
        val controller = RecordingController(ViewerEngineSnapshot())
        val viewModel = ViewerViewModel(controller, InMemoryMobileSettingsStore())

        viewModel.onEvent(
            ViewerUiEvent.SafRootGranted(
                uriToken = "content://documents/tree/new",
                requestRead = true,
                requestWrite = false,
            ),
        )
        advanceUntilIdle()

        assertThat(controller.safRequests).containsExactly(
            Triple("content://documents/tree/new", true, false),
        )
    }

    private fun exportController() = RecordingController(
        ViewerEngineSnapshot(
            currentDirectoryId = "directory",
            currentCapabilities = FilerCapabilitiesUi(canCreate = true),
            supportedExportFormats = setOf(
                ExportFormat.PNG,
                ExportFormat.JPEG,
                ExportFormat.WEBP_LOSSY,
            ),
        ),
    )

    private class RecordingController(initial: ViewerEngineSnapshot) :
        MobileViewerController by EmptyMobileViewerController() {
        override val snapshot = MutableStateFlow(initial)
        val requests = mutableListOf<MangaSpreadRequest>()
        val dispatchedActions = mutableListOf<ViewerAction>()
        val exports = mutableListOf<Pair<ExportRequest, String?>>()
        var lastSmbInput: SmbConnectionInput? = null
        var lastCredentialInput: SmbCredentialInput? = null
        var navigateUpCalls = 0
        val safRequests = mutableListOf<Triple<String, Boolean, Boolean>>()
        var selectionGate: CompletableDeferred<Unit>? = null

        override suspend fun selectFilerEntry(id: String) {
            selectionGate?.await()
        }

        override suspend fun dispatch(action: ViewerAction) {
            dispatchedActions += action
        }

        override suspend fun exportCurrent(request: ExportRequest, uriToken: String?) {
            exports += request to uriToken
        }

        override suspend fun requestMangaSpread(request: MangaSpreadRequest) {
            requests += request
        }

        override suspend fun requestSmbShares(input: SmbConnectionInput) {
            lastSmbInput = input
        }

        override suspend fun reenterSmbCredential(input: SmbCredentialInput) {
            lastCredentialInput = input
        }

        override suspend fun navigateUp() {
            navigateUpCalls += 1
        }

        override suspend fun addSafRoot(
            uriToken: String,
            requestRead: Boolean,
            requestWrite: Boolean,
        ): Boolean {
            safRequests += Triple(uriToken, requestRead, requestWrite)
            return true
        }
    }
}
