package io.github.mith_mmk.wml2viewer.ui

import android.content.res.Configuration
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.components.FilerPane
import io.github.mith_mmk.wml2viewer.ui.components.FilerPaneMode
import io.github.mith_mmk.wml2viewer.ui.components.Filmstrip
import io.github.mith_mmk.wml2viewer.ui.components.ExportDialog
import io.github.mith_mmk.wml2viewer.ui.components.MobileSettingsScreen
import io.github.mith_mmk.wml2viewer.ui.components.ViewerSurface
import io.github.mith_mmk.wml2viewer.ui.components.consumeViewerInput
import io.github.mith_mmk.wml2viewer.ui.components.ViewerQuickMenu
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.MobileScreen
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.state.MobileViewerUiState
import io.github.mith_mmk.wml2viewer.ui.state.ViewerUiEvent
import io.github.mith_mmk.wml2viewer.ui.state.ViewerUiEffect
import io.github.mith_mmk.wml2viewer.ui.state.ViewerViewModel
import io.github.mith_mmk.wml2viewer.ui.theme.CinematicDarkTheme

data class MobileUiHostCallbacks(
    /** Launches ACTION_OPEN_DOCUMENT_TREE. Send the granted URI back as SafRootGranted. */
    val requestSafRoot: () -> Unit = {},
    /** Activity bridge for WindowCompat.setDecorFitsSystemWindows. */
    val applyEdgeToEdge: (Boolean) -> Unit = {},
    /** Keeps system-bar icon contrast aligned with Light/Dark/System appearance. */
    val applyDarkSystemBars: (Boolean) -> Unit = {},
    /** Requests Android 13+ notification permission immediately before a transfer is queued. */
    val requestTransferNotifications: () -> Unit = {},
    /** Launches ACTION_OPEN_DOCUMENT_TREE; export uses the same atomic SAF writer as the filer. */
    val requestCreateExportDocument: (ExportRequest) -> Unit = {},
)

@Composable
fun Wml2ViewerApp(
    viewModel: ViewerViewModel = viewModel(),
    hostCallbacks: MobileUiHostCallbacks = MobileUiHostCallbacks(),
) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is ViewerUiEffect.CreateExportDocument ->
                    hostCallbacks.requestCreateExportDocument(effect.request)
            }
        }
    }
    MobileViewerContent(
        state = state,
        onEvent = viewModel::onEvent,
        hostCallbacks = hostCallbacks,
    )
}

/** Stateless entry point used by previews, UI tests, and platform-specific hosts. */
@Composable
fun MobileViewerContent(
    state: MobileViewerUiState,
    onEvent: (ViewerUiEvent) -> Unit,
    modifier: Modifier = Modifier,
    hostCallbacks: MobileUiHostCallbacks = MobileUiHostCallbacks(),
) {
    val context = LocalContext.current
    val hostView = LocalView.current
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    // A phone remains the compact mobile experience after rotation. Using only
    // current width would classify a Pixel phone in landscape as a tablet and
    // leave the filer/menu permanently beside the viewer.
    val isTablet = configuration.smallestScreenWidthDp >= 600
    val darkSystemBars = state.settings.theme == ThemeMode.CINEMATIC_DARK ||
        (state.settings.theme == ThemeMode.SYSTEM && isSystemInDarkTheme())
    val filerListState = rememberSaveable(saver = LazyListState.Saver) { LazyListState() }
    val folderListState = rememberSaveable(saver = LazyListState.Saver) { LazyListState() }
    val filmstripListState = rememberSaveable(saver = LazyListState.Saver) { LazyListState() }
    LaunchedEffect(state.settings.language) {
        AndroidLanguageController.apply(context, state.settings.language)
    }
    DisposableEffect(hostView, state.settings.viewing.keepScreenOn) {
        val previous = hostView.keepScreenOn
        hostView.keepScreenOn = state.settings.viewing.keepScreenOn
        onDispose { hostView.keepScreenOn = previous }
    }
    LaunchedEffect(state.settings.viewing.edgeToEdge, darkSystemBars) {
        hostCallbacks.applyDarkSystemBars(darkSystemBars)
        hostCallbacks.applyEdgeToEdge(state.settings.viewing.edgeToEdge)
    }
    val dispatch: (ViewerUiEvent) -> Unit = { event ->
        when (event) {
            ViewerUiEvent.RequestSafRoot -> hostCallbacks.requestSafRoot()
            ViewerUiEvent.CompleteFilerTransfer -> {
                hostCallbacks.requestTransferNotifications()
                onEvent(event)
            }
            else -> onEvent(event)
        }
    }
    CinematicDarkTheme(
        textScale = state.settings.textScale,
        themeMode = state.settings.theme,
        dynamicColor = state.settings.dynamicColor,
    ) {
        BoxWithConstraints(
            // Keep an explicit test/host size authoritative. App hosts without a
            // size still receive a full-screen root via fillMaxSize first.
            modifier = Modifier.fillMaxSize().then(modifier),
        ) {
            val compact = maxWidth < 600.dp || !isTablet
            val resolvedState = if (
                state.deviceClass == (
                    if (compact) DeviceClass.COMPACT else DeviceClass.EXPANDED
                ) &&
                state.isLandscape == isLandscape
            ) state else state.copy(
                deviceClass = if (compact) DeviceClass.COMPACT else DeviceClass.EXPANDED,
                isLandscape = isLandscape,
                touchReady = if (state.engine.mangaPages.isEmpty()) state.touchReady else false,
            )

            LaunchedEffect(maxWidth, isLandscape, isTablet) {
                dispatch(ViewerUiEvent.WindowMetricsChanged(maxWidth.value, isLandscape, isTablet))
            }
            BackHandler(
                enabled = resolvedState.screen != MobileScreen.VIEWER ||
                    resolvedState.subfilerVisible || resolvedState.quickMenuVisible ||
                    resolvedState.exportDialogVisible,
            ) {
                dispatch(ViewerUiEvent.Back)
            }

            if (compact) {
                CompactContent(
                    state = resolvedState,
                    onEvent = dispatch,
                    filerListState = filerListState,
                    folderListState = folderListState,
                    filmstripListState = filmstripListState,
                    useLandscapeFiler = isLandscape && !isTablet,
                )
            } else {
                ExpandedContent(
                    resolvedState,
                    dispatch,
                    folderListState,
                    filerListState,
                    filmstripListState,
                    navigationWidth = if (maxWidth >= 840.dp) 300.dp else 240.dp,
                )
            }
        }
    }
}

@Composable
private fun CompactContent(
    state: MobileViewerUiState,
    onEvent: (ViewerUiEvent) -> Unit,
    filerListState: LazyListState,
    folderListState: LazyListState,
    filmstripListState: LazyListState,
    useLandscapeFiler: Boolean,
) {
    when (state.screen) {
        MobileScreen.FILER -> if (useLandscapeFiler) {
            LandscapeFilerContent(state, onEvent, folderListState, filerListState)
        } else FilerPane(
            entries = state.engine.filerEntries,
            selectedSource = state.engine.sourceKind,
            pathLabel = state.engine.pathLabel,
            currentDirectoryId = state.engine.currentDirectoryId,
            breadcrumb = state.engine.breadcrumb,
            currentCapabilities = state.engine.currentCapabilities,
            pendingTransfer = state.pendingTransfer,
            pendingCollision = state.engine.pendingCollision,
            availableSmbShares = state.engine.availableSmbShares,
            smbSharesLoading = state.engine.smbSharesLoading,
            smbSetupId = state.engine.smbSetupId,
            smbSecurityStatus = state.engine.smbSecurityStatus,
            compact = true,
            onBack = { onEvent(ViewerUiEvent.Back) },
            onClose = { onEvent(ViewerUiEvent.CloseFiler) },
            onSelectSource = { onEvent(ViewerUiEvent.SelectSource(it)) },
            onRefresh = { onEvent(ViewerUiEvent.RefreshFiler) },
            onSelectEntry = {
                onEvent(ViewerUiEvent.SelectFilerEntry(it.id, it.isContainer))
            },
            onNavigateToBreadcrumb = { onEvent(ViewerUiEvent.NavigateToBreadcrumb(it)) },
            onRequestSafRoot = { onEvent(ViewerUiEvent.RequestSafRoot) },
            onRequestSmbShares = { onEvent(ViewerUiEvent.RequestSmbShares(it)) },
            onAddSmbSource = { onEvent(ViewerUiEvent.AddSmbSource(it)) },
            onReenterSmbCredential = { onEvent(ViewerUiEvent.ReenterSmbCredential(it)) },
            onForgetSmbSource = { onEvent(ViewerUiEvent.ForgetSmbSource(it)) },
            onBeginTransfer = { onEvent(ViewerUiEvent.BeginFilerTransfer(it)) },
            onCancelTransfer = { onEvent(ViewerUiEvent.CancelFilerTransfer) },
            onCompleteTransfer = { onEvent(ViewerUiEvent.CompleteFilerTransfer) },
            onOperation = { onEvent(ViewerUiEvent.PerformFilerOperation(it)) },
            onResolveCollision = { id, resolution, all ->
                onEvent(ViewerUiEvent.ResolveCollision(id, resolution, all))
            },
            listState = filerListState,
        )
        MobileScreen.SETTINGS -> SettingsContent(state, compact = true, onEvent)
        MobileScreen.VIEWER -> ViewerPane(
            state,
            compact = true,
            onEvent = onEvent,
            filmstripListState = filmstripListState,
        )
    }
}

@Composable
private fun LandscapeFilerContent(
    state: MobileViewerUiState,
    onEvent: (ViewerUiEvent) -> Unit,
    folderListState: LazyListState,
    filerListState: LazyListState,
) {
    Row(
        modifier = Modifier
            .fillMaxSize()
            .testTag("landscape-filer-two-pane"),
    ) {
        FilerPane(
            entries = state.engine.filerEntries.filter(FilerEntryUi::isContainer),
            selectedSource = state.engine.sourceKind,
            pathLabel = state.engine.pathLabel,
            currentDirectoryId = state.engine.currentDirectoryId,
            breadcrumb = state.engine.breadcrumb,
            currentCapabilities = state.engine.currentCapabilities,
            pendingTransfer = state.pendingTransfer,
            pendingCollision = state.engine.pendingCollision,
            availableSmbShares = state.engine.availableSmbShares,
            smbSharesLoading = state.engine.smbSharesLoading,
            smbSetupId = state.engine.smbSetupId,
            smbSecurityStatus = state.engine.smbSecurityStatus,
            compact = true,
            onBack = { onEvent(ViewerUiEvent.Back) },
            onClose = { onEvent(ViewerUiEvent.CloseFiler) },
            onSelectSource = { onEvent(ViewerUiEvent.SelectSource(it)) },
            onRefresh = { onEvent(ViewerUiEvent.RefreshFiler) },
            onSelectEntry = {
                onEvent(ViewerUiEvent.SelectFilerEntry(it.id, it.isContainer))
            },
            onNavigateToBreadcrumb = { onEvent(ViewerUiEvent.NavigateToBreadcrumb(it)) },
            onRequestSafRoot = { onEvent(ViewerUiEvent.RequestSafRoot) },
            onRequestSmbShares = { onEvent(ViewerUiEvent.RequestSmbShares(it)) },
            onAddSmbSource = { onEvent(ViewerUiEvent.AddSmbSource(it)) },
            onReenterSmbCredential = { onEvent(ViewerUiEvent.ReenterSmbCredential(it)) },
            onForgetSmbSource = { onEvent(ViewerUiEvent.ForgetSmbSource(it)) },
            onBeginTransfer = { onEvent(ViewerUiEvent.BeginFilerTransfer(it)) },
            onCancelTransfer = { onEvent(ViewerUiEvent.CancelFilerTransfer) },
            onCompleteTransfer = { onEvent(ViewerUiEvent.CompleteFilerTransfer) },
            onOperation = { onEvent(ViewerUiEvent.PerformFilerOperation(it)) },
            onResolveCollision = { id, resolution, all ->
                onEvent(ViewerUiEvent.ResolveCollision(id, resolution, all))
            },
            listState = folderListState,
            mode = FilerPaneMode.NAVIGATION,
            paneTag = "landscape-filer-navigation",
            modifier = Modifier.weight(0.36f),
        )
        FilerPane(
            entries = state.engine.filerEntries.filterNot(FilerEntryUi::isContainer),
            selectedSource = state.engine.sourceKind,
            pathLabel = state.engine.pathLabel,
            currentDirectoryId = state.engine.currentDirectoryId,
            breadcrumb = state.engine.breadcrumb,
            currentCapabilities = state.engine.currentCapabilities,
            pendingTransfer = state.pendingTransfer,
            pendingCollision = state.engine.pendingCollision,
            availableSmbShares = state.engine.availableSmbShares,
            smbSharesLoading = state.engine.smbSharesLoading,
            smbSetupId = state.engine.smbSetupId,
            smbSecurityStatus = state.engine.smbSecurityStatus,
            compact = false,
            onBack = { onEvent(ViewerUiEvent.Back) },
            onSelectSource = { onEvent(ViewerUiEvent.SelectSource(it)) },
            onRefresh = { onEvent(ViewerUiEvent.RefreshFiler) },
            onSelectEntry = {
                onEvent(ViewerUiEvent.SelectFilerEntry(it.id, it.isContainer))
            },
            onNavigateToBreadcrumb = { onEvent(ViewerUiEvent.NavigateToBreadcrumb(it)) },
            onRequestSafRoot = { onEvent(ViewerUiEvent.RequestSafRoot) },
            onRequestSmbShares = { onEvent(ViewerUiEvent.RequestSmbShares(it)) },
            onAddSmbSource = { onEvent(ViewerUiEvent.AddSmbSource(it)) },
            onReenterSmbCredential = { onEvent(ViewerUiEvent.ReenterSmbCredential(it)) },
            onForgetSmbSource = { onEvent(ViewerUiEvent.ForgetSmbSource(it)) },
            onBeginTransfer = { onEvent(ViewerUiEvent.BeginFilerTransfer(it)) },
            onCancelTransfer = { onEvent(ViewerUiEvent.CancelFilerTransfer) },
            onCompleteTransfer = { onEvent(ViewerUiEvent.CompleteFilerTransfer) },
            onOperation = { onEvent(ViewerUiEvent.PerformFilerOperation(it)) },
            onResolveCollision = { id, resolution, all ->
                onEvent(ViewerUiEvent.ResolveCollision(id, resolution, all))
            },
            listState = filerListState,
            navigationControls = false,
            mode = FilerPaneMode.LIST,
            paneTag = "landscape-filer-list",
            modifier = Modifier.weight(0.64f),
        )
    }
}

@Composable
private fun ExpandedContent(
    state: MobileViewerUiState,
    onEvent: (ViewerUiEvent) -> Unit,
    folderListState: LazyListState,
    fileListState: LazyListState,
    filmstripListState: LazyListState,
    navigationWidth: Dp,
) {
    Row(Modifier.fillMaxSize().testTag("expanded-two-pane")) {
        FilerPane(
            entries = state.engine.filerEntries.filter(FilerEntryUi::isContainer),
            selectedSource = state.engine.sourceKind,
            pathLabel = state.engine.pathLabel,
            currentDirectoryId = state.engine.currentDirectoryId,
            breadcrumb = state.engine.breadcrumb,
            currentCapabilities = state.engine.currentCapabilities,
            pendingTransfer = state.pendingTransfer,
            pendingCollision = state.engine.pendingCollision,
            availableSmbShares = state.engine.availableSmbShares,
            smbSharesLoading = state.engine.smbSharesLoading,
            smbSetupId = state.engine.smbSetupId,
            smbSecurityStatus = state.engine.smbSecurityStatus,
            compact = false,
            onBack = {},
            onSelectSource = { onEvent(ViewerUiEvent.SelectSource(it)) },
            onRefresh = { onEvent(ViewerUiEvent.RefreshFiler) },
            onSelectEntry = {
                onEvent(ViewerUiEvent.SelectFilerEntry(it.id, it.isContainer))
            },
            onNavigateToBreadcrumb = { onEvent(ViewerUiEvent.NavigateToBreadcrumb(it)) },
            onRequestSafRoot = { onEvent(ViewerUiEvent.RequestSafRoot) },
            onRequestSmbShares = { onEvent(ViewerUiEvent.RequestSmbShares(it)) },
            onAddSmbSource = { onEvent(ViewerUiEvent.AddSmbSource(it)) },
            onReenterSmbCredential = { onEvent(ViewerUiEvent.ReenterSmbCredential(it)) },
            onForgetSmbSource = { onEvent(ViewerUiEvent.ForgetSmbSource(it)) },
            onBeginTransfer = { onEvent(ViewerUiEvent.BeginFilerTransfer(it)) },
            onCancelTransfer = { onEvent(ViewerUiEvent.CancelFilerTransfer) },
            onCompleteTransfer = { onEvent(ViewerUiEvent.CompleteFilerTransfer) },
            onOperation = { onEvent(ViewerUiEvent.PerformFilerOperation(it)) },
            onResolveCollision = { id, resolution, all ->
                onEvent(ViewerUiEvent.ResolveCollision(id, resolution, all))
            },
            listState = folderListState,
            modifier = Modifier.width(navigationWidth),
        )
        when (state.screen) {
            MobileScreen.SETTINGS -> SettingsContent(state, compact = false, onEvent, Modifier.weight(1f))
            MobileScreen.FILER -> FilerPane(
                entries = state.engine.filerEntries.filterNot(FilerEntryUi::isContainer),
                selectedSource = state.engine.sourceKind,
                pathLabel = state.engine.pathLabel,
                currentDirectoryId = state.engine.currentDirectoryId,
                breadcrumb = state.engine.breadcrumb,
                currentCapabilities = state.engine.currentCapabilities,
                pendingTransfer = state.pendingTransfer,
                pendingCollision = state.engine.pendingCollision,
                availableSmbShares = state.engine.availableSmbShares,
                smbSharesLoading = state.engine.smbSharesLoading,
                smbSetupId = state.engine.smbSetupId,
                smbSecurityStatus = state.engine.smbSecurityStatus,
                compact = false,
                onBack = {},
                onSelectSource = { onEvent(ViewerUiEvent.SelectSource(it)) },
                onRefresh = { onEvent(ViewerUiEvent.RefreshFiler) },
                onSelectEntry = {
                    onEvent(ViewerUiEvent.SelectFilerEntry(it.id, it.isContainer))
                },
                onNavigateToBreadcrumb = { onEvent(ViewerUiEvent.NavigateToBreadcrumb(it)) },
                onRequestSafRoot = { onEvent(ViewerUiEvent.RequestSafRoot) },
                onRequestSmbShares = { onEvent(ViewerUiEvent.RequestSmbShares(it)) },
                onAddSmbSource = { onEvent(ViewerUiEvent.AddSmbSource(it)) },
                onReenterSmbCredential = { onEvent(ViewerUiEvent.ReenterSmbCredential(it)) },
                onForgetSmbSource = { onEvent(ViewerUiEvent.ForgetSmbSource(it)) },
                onBeginTransfer = { onEvent(ViewerUiEvent.BeginFilerTransfer(it)) },
                onCancelTransfer = { onEvent(ViewerUiEvent.CancelFilerTransfer) },
                onCompleteTransfer = { onEvent(ViewerUiEvent.CompleteFilerTransfer) },
                onOperation = { onEvent(ViewerUiEvent.PerformFilerOperation(it)) },
                onResolveCollision = { id, resolution, all ->
                    onEvent(ViewerUiEvent.ResolveCollision(id, resolution, all))
                },
                listState = fileListState,
                navigationControls = false,
                paneTag = "tablet-list-pane",
                modifier = Modifier.weight(1f),
            )
            MobileScreen.VIEWER -> ViewerPane(
                state,
                compact = false,
                onEvent = onEvent,
                filmstripListState = filmstripListState,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun SettingsContent(
    state: MobileViewerUiState,
    compact: Boolean,
    onEvent: (ViewerUiEvent) -> Unit,
    modifier: Modifier = Modifier,
) {
    MobileSettingsScreen(
        settings = state.settings,
        measuredOsCodecFormats = state.engine.measuredOsCodecFormats,
        selectedCategory = state.selectedSettingsCategory,
        compact = compact,
        onBack = { onEvent(ViewerUiEvent.Back) },
        onSelectCategory = { onEvent(ViewerUiEvent.SelectSettingsCategory(it)) },
        onSettingsChange = { onEvent(ViewerUiEvent.ReplaceSettings(it)) },
        modifier = modifier,
    )
}

@Composable
private fun ViewerPane(
    state: MobileViewerUiState,
    compact: Boolean,
    onEvent: (ViewerUiEvent) -> Unit,
    filmstripListState: LazyListState,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxSize().testTag("viewer-pane")) {
        ViewerSurface(
            state = state,
            gestureSettings = state.settings.gestures,
            onZoneTap = { onEvent(ViewerUiEvent.TapZonePressed(it)) },
            onAction = { onEvent(ViewerUiEvent.PerformAction(it)) },
            onTransform = { panX, panY, zoom ->
                onEvent(ViewerUiEvent.Transform(panX, panY, zoom))
            },
            onViewportSizeChanged = { size ->
                onEvent(ViewerUiEvent.ViewportSizeChanged(size.width, size.height))
            },
        )
        if (state.settings.viewing.showTopChrome && !(compact && state.isLandscape)) {
            Surface(
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth()
                    .testTag("viewer-top-chrome")
                    .consumeViewerInput()
                    .windowInsetsPadding(
                        WindowInsets.safeDrawing.only(
                            WindowInsetsSides.Top + WindowInsetsSides.Horizontal,
                        ),
                    ),
                color = MaterialTheme.colorScheme.surface.copy(alpha = 0.96f),
                tonalElevation = 6.dp,
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (compact) {
                        TextButton(
                            onClick = {
                                onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_FILER))
                            },
                        ) {
                            Text(stringResource(R.string.viewer_open_filer))
                        }
                    }
                    Text(
                        text = state.engine.title.ifBlank { stringResource(R.string.app_name) },
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(
                        onClick = {
                            onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_SUBFILER))
                        },
                    ) {
                        Text(stringResource(R.string.viewer_open_subfiler))
                    }
                    TextButton(
                        onClick = {
                            onEvent(ViewerUiEvent.PerformAction(ViewerAction.OPEN_SETTINGS))
                        },
                    ) {
                        Text(stringResource(R.string.viewer_open_settings))
                    }
                }
            }
        }
        if (
            (state.subfilerVisible || (!compact && state.settings.filer.tabletFilmstripPinned)) &&
            state.settings.viewing.showFilmstrip
        ) {
            Filmstrip(
                items = state.engine.filmstrip,
                onSelect = { onEvent(ViewerUiEvent.SelectFilmstripItem(it)) },
                listState = filmstripListState,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }
        if (state.quickMenuVisible) {
            ViewerQuickMenu(
                exportEnabled = state.engine.supportedExportFormats.isNotEmpty() &&
                    !state.engine.exporting,
                onAction = { action ->
                    onEvent(ViewerUiEvent.DismissQuickMenu)
                    onEvent(ViewerUiEvent.PerformAction(action))
                },
                onDismiss = { onEvent(ViewerUiEvent.DismissQuickMenu) },
            )
        }
        if (state.exportDialogVisible) {
            ExportDialog(
                supportedFormats = state.engine.supportedExportFormats,
                initialTitle = state.engine.title,
                canCreateCurrentDirectory = state.engine.currentCapabilities.canCreate &&
                    state.engine.currentDirectoryId != null,
                exporting = state.engine.exporting,
                onConfirm = { onEvent(ViewerUiEvent.SubmitExport(it)) },
                onDismiss = { onEvent(ViewerUiEvent.DismissExportDialog) },
            )
        }
    }
}
