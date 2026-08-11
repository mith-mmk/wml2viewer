package io.github.mith_mmk.wml2viewer.ui.state

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.MobileScreen
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class ViewerViewModel(
    private val controller: MobileViewerController = EmptyMobileViewerController(),
    private val settingsStore: MobileSettingsStore = InMemoryMobileSettingsStore(),
) : ViewModel() {
    private val _uiState = MutableStateFlow(
        MobileViewerUiState(
            settings = settingsStore.settings.value,
            engine = controller.snapshot.value,
        ),
    )
    val uiState: StateFlow<MobileViewerUiState> = _uiState.asStateFlow()
    private val mutableEffects = Channel<ViewerUiEffect>(capacity = Channel.BUFFERED)
    val effects = mutableEffects.receiveAsFlow()
    private var pendingSystemExport: ExportRequest? = null

    init {
        viewModelScope.launch {
            controller.snapshot.collect { snapshot ->
                val previous = _uiState.value.engine
                val mangaIdentityChanged =
                    previous.currentLogicalPageIndex != snapshot.currentLogicalPageIndex ||
                    previous.mangaPages != snapshot.mangaPages
                _uiState.update {
                    it.copy(
                        engine = snapshot,
                        fitOverride = if (mangaIdentityChanged) null else it.fitOverride,
                    )
                }
                if (mangaIdentityChanged) requestMangaSpread(_uiState.value.isLandscape)
            }
        }
        viewModelScope.launch {
            settingsStore.settings.collect { settings ->
                val mangaChanged = _uiState.value.settings.manga != settings.manga
                _uiState.update { it.copy(settings = settings) }
                if (mangaChanged) requestMangaSpread(_uiState.value.isLandscape)
            }
        }
    }

    fun onEvent(event: ViewerUiEvent) {
        when (event) {
            is ViewerUiEvent.WindowMetricsChanged -> updateWindowMetrics(event)
            ViewerUiEvent.Back -> navigateBack()
            is ViewerUiEvent.TapZonePressed -> performAction(
                _uiState.value.settings.touchMap.actionFor(event.zone),
            )
            is ViewerUiEvent.PerformAction -> performAction(event.action)
            is ViewerUiEvent.SelectSettingsCategory -> _uiState.update {
                it.copy(selectedSettingsCategory = event.category)
            }
            is ViewerUiEvent.SelectFilerEntry -> {
                // Leave the filer as soon as a file is selected. Decoding can involve
                // SAF/SMB I/O and must not leave the user's tap target covered while
                // that work is in progress. The viewer shows its loading/error state
                // until the controller publishes the decoded page.
                if (!event.isContainer) {
                    _uiState.update { it.copy(screen = MobileScreen.VIEWER) }
                }
                viewModelScope.launch {
                    controller.selectFilerEntry(event.id)
                    if (event.isContainer) {
                        _uiState.update { it.copy(screen = MobileScreen.FILER) }
                    }
                }
            }
            ViewerUiEvent.NavigateUp -> viewModelScope.launch {
                controller.navigateUp()
                if (_uiState.value.deviceClass != DeviceClass.COMPACT) {
                    _uiState.update { it.copy(screen = MobileScreen.FILER) }
                }
            }
            is ViewerUiEvent.NavigateToBreadcrumb -> viewModelScope.launch {
                controller.navigateToBreadcrumb(event.id)
                if (_uiState.value.deviceClass != DeviceClass.COMPACT) {
                    _uiState.update { it.copy(screen = MobileScreen.FILER) }
                }
            }
            ViewerUiEvent.RequestSafRoot -> Unit
            is ViewerUiEvent.SafRootGranted -> viewModelScope.launch {
                controller.addSafRoot(event.uriToken, event.requestRead, event.requestWrite)
            }
            is ViewerUiEvent.RequestSmbShares -> {
                val owned = takeSmbInput(event.input)
                viewModelScope.launch {
                    submitSmbInput(owned, controller::requestSmbShares)
                }
            }
            is ViewerUiEvent.AddSmbSource -> {
                val owned = takeSmbInput(event.input)
                viewModelScope.launch { submitSmbInput(owned, controller::addSmbSource) }
            }
            is ViewerUiEvent.ReenterSmbCredential -> {
                val owned = takeSmbCredentialInput(event.input)
                viewModelScope.launch {
                    try {
                        controller.reenterSmbCredential(owned)
                    } finally {
                        owned.clearPassword()
                    }
                }
            }
            is ViewerUiEvent.ForgetSmbSource -> viewModelScope.launch {
                controller.forgetSmbSource(event.sourceId)
            }
            is ViewerUiEvent.BeginFilerTransfer -> _uiState.update {
                it.copy(pendingTransfer = event.transfer)
            }
            ViewerUiEvent.CancelFilerTransfer -> _uiState.update {
                it.copy(pendingTransfer = null)
            }
            ViewerUiEvent.CompleteFilerTransfer -> completeFilerTransfer()
            is ViewerUiEvent.PerformFilerOperation -> viewModelScope.launch {
                controller.performFilerOperation(event.request)
            }
            is ViewerUiEvent.ResolveCollision -> viewModelScope.launch {
                controller.resolveCollision(
                    event.operationId,
                    event.resolution,
                    event.applyToAll,
                )
            }
            is ViewerUiEvent.SelectFilmstripItem -> viewModelScope.launch {
                controller.selectFilmstripItem(event.id)
            }
            is ViewerUiEvent.SelectSource -> viewModelScope.launch {
                controller.selectSource(event.source)
                if (_uiState.value.deviceClass != DeviceClass.COMPACT) {
                    _uiState.update { it.copy(screen = MobileScreen.FILER) }
                }
            }
            ViewerUiEvent.RefreshFiler -> viewModelScope.launch { controller.refreshFiler() }
            is ViewerUiEvent.Transform -> applyTransform(event)
            is ViewerUiEvent.ReplaceSettings -> replaceSettings(event.settings)
            ViewerUiEvent.RefreshMangaSpread -> requestMangaSpread(_uiState.value.isLandscape)
            ViewerUiEvent.DismissQuickMenu -> _uiState.update { it.copy(quickMenuVisible = false) }
            ViewerUiEvent.DismissExportDialog -> _uiState.update {
                it.copy(exportDialogVisible = false)
            }
            is ViewerUiEvent.SubmitExport -> submitExport(event.request)
            is ViewerUiEvent.ExportDocumentCreated -> completeSystemExport(
                event.uriToken,
                event.restoredRequest,
            )
            ViewerUiEvent.ExportDocumentCancelled -> pendingSystemExport = null
        }
    }

    private fun updateWindowMetrics(event: ViewerUiEvent.WindowMetricsChanged) {
        val deviceClass = if (event.widthDp < 600f) DeviceClass.COMPACT else DeviceClass.EXPANDED
        _uiState.update {
            it.copy(deviceClass = deviceClass, isLandscape = event.isLandscape)
        }
        requestMangaSpread(event.isLandscape)
    }

    private fun navigateBack() {
        if (_uiState.value.exportDialogVisible) {
            _uiState.update { it.copy(exportDialogVisible = false) }
            return
        }
        if (_uiState.value.screen == MobileScreen.FILER && !_uiState.value.engine.atSourceRoot) {
            viewModelScope.launch { controller.navigateUp() }
            return
        }
        _uiState.update { state ->
            when {
                state.quickMenuVisible -> state.copy(quickMenuVisible = false)
                state.screen == MobileScreen.SETTINGS && state.selectedSettingsCategory != null ->
                    state.copy(selectedSettingsCategory = null)
                state.screen != MobileScreen.VIEWER -> state.copy(
                    screen = MobileScreen.VIEWER,
                    selectedSettingsCategory = null,
                )
                state.subfilerVisible -> state.copy(subfilerVisible = false)
                else -> state
            }
        }
    }

    private fun completeFilerTransfer() {
        val state = _uiState.value
        val transfer = state.pendingTransfer ?: return
        val destinationId = state.engine.currentDirectoryId ?: return
        _uiState.update { it.copy(pendingTransfer = null) }
        viewModelScope.launch {
            controller.performFilerOperation(
                io.github.mith_mmk.wml2viewer.ui.model.FilerOperationRequest(
                    type = transfer.type,
                    entryId = transfer.entryId,
                    destinationId = destinationId,
                ),
            )
        }
    }

    private fun performAction(action: ViewerAction) {
        when (action) {
            ViewerAction.NONE -> Unit
            ViewerAction.OPEN_FILER -> {
                if (_uiState.value.deviceClass == DeviceClass.COMPACT) {
                    _uiState.update { it.copy(screen = MobileScreen.FILER) }
                }
            }
            ViewerAction.OPEN_SETTINGS -> _uiState.update {
                it.copy(screen = MobileScreen.SETTINGS, selectedSettingsCategory = null)
            }
            ViewerAction.OPEN_SUBFILER -> _uiState.update {
                it.copy(subfilerVisible = !it.subfilerVisible)
            }
            ViewerAction.OPEN_CONTEXT_MENU -> _uiState.update {
                it.copy(quickMenuVisible = true)
            }
            ViewerAction.TOGGLE_FIT_MODE -> {
                val current = _uiState.value
                val effectiveFit = current.fitOverride ?: current.settings.viewing.fit
                val fit = if (effectiveFit == DisplayFit.CONTAIN) {
                    DisplayFit.ORIGINAL
                } else {
                    DisplayFit.CONTAIN
                }
                _uiState.update {
                    it.copy(fitOverride = fit).withViewport(ViewerViewportReducer.reset())
                }
            }
            ViewerAction.ZOOM_RESET -> _uiState.update {
                it.withViewport(ViewerViewportReducer.reset())
            }
            ViewerAction.ZOOM_IN -> updateViewport(ViewerViewportReducer::zoomIn)
            ViewerAction.ZOOM_OUT -> updateViewport(ViewerViewportReducer::zoomOut)
            ViewerAction.TOGGLE_GRAYSCALE -> _uiState.update {
                it.copy(grayscaleEnabled = !it.grayscaleEnabled)
            }
            ViewerAction.EXPORT -> {
                val state = _uiState.value
                if (state.engine.supportedExportFormats.isNotEmpty() &&
                    !state.engine.exporting && pendingSystemExport == null
                ) {
                    _uiState.update { it.copy(exportDialogVisible = true) }
                }
            }
            else -> viewModelScope.launch { controller.dispatch(action) }
        }
    }

    private fun submitExport(request: ExportRequest) {
        val state = _uiState.value
        if (request.format !in state.engine.supportedExportFormats ||
            state.engine.exporting || pendingSystemExport != null
        ) return
        if (request.destination == ExportDestination.CURRENT_DIRECTORY &&
            (!state.engine.currentCapabilities.canCreate || state.engine.currentDirectoryId == null)
        ) return
        _uiState.update { it.copy(exportDialogVisible = false) }
        when (request.destination) {
            ExportDestination.CURRENT_DIRECTORY -> viewModelScope.launch {
                controller.exportCurrent(request)
            }
            ExportDestination.SYSTEM_PICKER -> {
                pendingSystemExport = request
                mutableEffects.trySend(ViewerUiEffect.CreateExportDocument(request))
            }
        }
    }

    private fun completeSystemExport(uriToken: String, restoredRequest: ExportRequest?) {
        val request = pendingSystemExport ?: restoredRequest ?: return
        pendingSystemExport = null
        if (uriToken.isBlank()) return
        viewModelScope.launch { controller.exportCurrent(request, uriToken) }
    }

    private fun applyTransform(event: ViewerUiEvent.Transform) {
        updateViewport { viewport ->
            ViewerViewportReducer.transform(
                viewport = viewport,
                panX = event.panX,
                panY = event.panY,
                zoomChange = event.zoomChange,
            )
        }
    }

    private fun updateViewport(transform: (ViewerViewport) -> ViewerViewport) {
        _uiState.update { state -> state.withViewport(transform(state.viewport())) }
    }

    private fun replaceSettings(settings: MobileViewerSettings) {
        val mangaChanged = _uiState.value.settings.manga != settings.manga
        _uiState.update { it.copy(settings = settings) }
        viewModelScope.launch { settingsStore.replace(settings) }
        if (mangaChanged) requestMangaSpread(_uiState.value.isLandscape)
    }

    private fun requestMangaSpread(isLandscape: Boolean) {
        val state = _uiState.value
        val currentIndex = state.engine.currentLogicalPageIndex
        if (currentIndex !in state.engine.mangaPages.indices) return
        viewModelScope.launch {
            controller.requestMangaSpread(
                MangaSpreadRequest(
                    currentLogicalPageIndex = currentIndex,
                    landscape = isLandscape,
                    divider = state.settings.manga.divider,
                    prefetchSpreads = state.settings.manga.prefetchSpreads,
                ),
            )
        }
    }

    private suspend fun submitSmbInput(
        input: io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput,
        submit: suspend (io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput) -> Unit,
    ) {
        try {
            submit(input)
        } finally {
            input.clearPassword()
        }
    }

    private fun takeSmbInput(
        input: io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput,
    ): io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput {
        val passwordCopy = input.password.copyOf()
        return try {
            input.copy(password = passwordCopy)
        } catch (error: Throwable) {
            passwordCopy.fill('\u0000')
            throw error
        } finally {
            input.clearPassword()
        }
    }

    private fun takeSmbCredentialInput(
        input: io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput,
    ): io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput {
        val passwordCopy = input.password.copyOf()
        return try {
            input.copy(password = passwordCopy)
        } catch (error: Throwable) {
            passwordCopy.fill('\u0000')
            throw error
        } finally {
            input.clearPassword()
        }
    }
}

private fun MobileViewerUiState.viewport(): ViewerViewport = ViewerViewport(
    zoom = zoom,
    panX = panX,
    panY = panY,
)

private fun MobileViewerUiState.withViewport(viewport: ViewerViewport): MobileViewerUiState = copy(
    zoom = viewport.zoom,
    panX = viewport.panX,
    panY = viewport.panY,
)

/** ServiceLocator/AppGraph integration point for production Activity hosts. */
class ViewerViewModelFactory(
    private val controller: MobileViewerController,
    private val settingsStore: MobileSettingsStore,
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        require(modelClass.isAssignableFrom(ViewerViewModel::class.java)) {
            "Unsupported ViewModel type: ${modelClass.name}"
        }
        return ViewerViewModel(controller, settingsStore) as T
    }
}
