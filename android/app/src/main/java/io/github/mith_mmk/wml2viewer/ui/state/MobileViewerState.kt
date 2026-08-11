package io.github.mith_mmk.wml2viewer.ui.state

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.ImageBitmap
import io.github.mith_mmk.wml2viewer.ui.model.DeviceClass
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.model.FilmstripItemUi
import io.github.mith_mmk.wml2viewer.ui.model.BreadcrumbUi
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.FilerCapabilitiesUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationRequest
import io.github.mith_mmk.wml2viewer.ui.model.PendingCollisionUi
import io.github.mith_mmk.wml2viewer.ui.model.PendingTransferUi
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbSecurityStatusUi
import io.github.mith_mmk.wml2viewer.ui.model.MobileScreen
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.MangaPageRef
import io.github.mith_mmk.wml2viewer.ui.model.SettingsCategory
import io.github.mith_mmk.wml2viewer.ui.model.SourceKind
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction

enum class UiErrorCode {
    INVALID_HANDLE,
    INVALID_REQUEST,
    STALE_REQUEST,
    CANCELLED,
    IO,
    DECODE,
    ENCODE,
    OS_ANIMATION_UNSUPPORTED,
    LIMIT,
    AUTHENTICATION_FAILED,
    ACCESS_DENIED,
    NETWORK,
    INTEGRITY,
    PERMISSION_REVOKED,
    KEYSTORE_INVALIDATED,
    UNKNOWN,
}

/** Keeps Rust ABI integers out of the platform/UI error identity. */
object NativeUiErrorMapper {
    fun fromCode(code: Int): UiErrorCode = when (code) {
        1 -> UiErrorCode.INVALID_HANDLE
        2 -> UiErrorCode.INVALID_REQUEST
        3 -> UiErrorCode.STALE_REQUEST
        4 -> UiErrorCode.CANCELLED
        5 -> UiErrorCode.IO
        6 -> UiErrorCode.DECODE
        7 -> UiErrorCode.LIMIT
        8 -> UiErrorCode.ENCODE
        else -> UiErrorCode.UNKNOWN
    }
}

/** Localizable error identity. Args must already be scrubbed of paths and credentials. */
@Immutable
data class UiError(
    val code: UiErrorCode,
    val args: List<String> = emptyList(),
)

@Immutable
data class ViewerPageFrameUi(
    val page: MangaPageRef,
    val frame: ImageBitmap,
)

@Immutable
data class MangaSpreadRequest(
    val currentLogicalPageIndex: Int,
    val landscape: Boolean,
    val divider: Boolean,
    val prefetchSpreads: Int,
)

@Immutable
data class ViewerEngineSnapshot(
    val title: String = "",
    val frame: ImageBitmap? = null,
    val mangaPages: List<MangaPageRef> = emptyList(),
    val currentLogicalPageIndex: Int = 0,
    /** Zero to two composited page frames, in final display order. */
    val spreadFrames: List<ViewerPageFrameUi> = emptyList(),
    val filerEntries: List<FilerEntryUi> = emptyList(),
    val filmstrip: List<FilmstripItemUi> = emptyList(),
    val sourceKind: SourceKind = SourceKind.LOCAL,
    val pathLabel: String = "",
    val currentDirectoryId: String? = null,
    val breadcrumb: List<BreadcrumbUi> = emptyList(),
    val atSourceRoot: Boolean = true,
    val currentCapabilities: FilerCapabilitiesUi = FilerCapabilitiesUi(),
    val availableSmbShares: List<String> = emptyList(),
    val smbSharesLoading: Boolean = false,
    val smbSetupId: String? = null,
    val smbSecurityStatus: SmbSecurityStatusUi? = null,
    val pendingCollision: PendingCollisionUi? = null,
    /** Formats that passed the device OS codec round-trip probe at startup. */
    val measuredOsCodecFormats: Set<CodecFormat> = emptySet(),
    val supportedExportFormats: Set<ExportFormat> = emptySet(),
    val exporting: Boolean = false,
    val loading: Boolean = false,
    val error: UiError? = null,
) {
    init {
        require(spreadFrames.size <= 2) { "A manga spread contains at most two frames" }
    }
}

@Immutable
data class MobileViewerUiState(
    val deviceClass: DeviceClass = DeviceClass.COMPACT,
    val screen: MobileScreen = MobileScreen.VIEWER,
    val selectedSettingsCategory: SettingsCategory? = null,
    val isLandscape: Boolean = false,
    val subfilerVisible: Boolean = false,
    val settings: MobileViewerSettings = MobileViewerSettings(),
    val engine: ViewerEngineSnapshot = ViewerEngineSnapshot(),
    val fitOverride: DisplayFit? = null,
    val zoom: Float = 1f,
    val panX: Float = 0f,
    val panY: Float = 0f,
    val grayscaleEnabled: Boolean = false,
    val pendingTransfer: PendingTransferUi? = null,
    val quickMenuVisible: Boolean = false,
    val exportDialogVisible: Boolean = false,
)

sealed interface ViewerUiEffect {
    data class CreateExportDocument(val request: ExportRequest) : ViewerUiEffect
}

sealed interface ViewerUiEvent {
    data class WindowMetricsChanged(val widthDp: Float, val isLandscape: Boolean) : ViewerUiEvent
    data object Back : ViewerUiEvent
    data class TapZonePressed(val zone: TapZone) : ViewerUiEvent
    data class PerformAction(val action: ViewerAction) : ViewerUiEvent
    data class SelectSettingsCategory(val category: SettingsCategory?) : ViewerUiEvent
    data class SelectFilerEntry(val id: String, val isContainer: Boolean) : ViewerUiEvent
    data object NavigateUp : ViewerUiEvent
    data class NavigateToBreadcrumb(val id: String) : ViewerUiEvent
    data object RequestSafRoot : ViewerUiEvent
    data class SafRootGranted(
        val uriToken: String,
        val requestRead: Boolean,
        val requestWrite: Boolean,
    ) : ViewerUiEvent
    data class RequestSmbShares(val input: SmbConnectionInput) : ViewerUiEvent
    data class AddSmbSource(val input: SmbConnectionInput) : ViewerUiEvent
    data class ReenterSmbCredential(val input: SmbCredentialInput) : ViewerUiEvent
    data class ForgetSmbSource(val sourceId: String) : ViewerUiEvent
    data class BeginFilerTransfer(val transfer: PendingTransferUi) : ViewerUiEvent
    data object CancelFilerTransfer : ViewerUiEvent
    data object CompleteFilerTransfer : ViewerUiEvent
    data class PerformFilerOperation(val request: FilerOperationRequest) : ViewerUiEvent
    data class ResolveCollision(
        val operationId: String,
        val resolution: CollisionResolution,
        val applyToAll: Boolean,
    ) : ViewerUiEvent
    data class SelectFilmstripItem(val id: String) : ViewerUiEvent
    data class SelectSource(val source: SourceKind) : ViewerUiEvent
    data object RefreshFiler : ViewerUiEvent
    data class Transform(val panX: Float, val panY: Float, val zoomChange: Float) : ViewerUiEvent
    data class ReplaceSettings(val settings: MobileViewerSettings) : ViewerUiEvent
    data object RefreshMangaSpread : ViewerUiEvent
    data object DismissQuickMenu : ViewerUiEvent
    data object DismissExportDialog : ViewerUiEvent
    data class SubmitExport(val request: ExportRequest) : ViewerUiEvent
    data class ExportDocumentCreated(
        val uriToken: String,
        /** Supplied by the Activity after process recreation while the system picker was open. */
        val restoredRequest: ExportRequest? = null,
    ) : ViewerUiEvent
    data object ExportDocumentCancelled : ViewerUiEvent
}
