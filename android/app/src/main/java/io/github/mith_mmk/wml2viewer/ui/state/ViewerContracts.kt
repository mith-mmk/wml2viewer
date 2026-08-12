package io.github.mith_mmk.wml2viewer.ui.state

import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationRequest
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import io.github.mith_mmk.wml2viewer.ui.model.SourceKind
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** UI-facing contract. Data/source implementations remain outside the Compose package. */
interface MobileViewerController {
    val snapshot: StateFlow<ViewerEngineSnapshot>
    suspend fun dispatch(action: ViewerAction)
    suspend fun selectFilerEntry(id: String)
    suspend fun navigateUp()
    suspend fun navigateToBreadcrumb(id: String)
    /** Atomically persists the URI grant and durable provider profile. */
    suspend fun addSafRoot(uriToken: String, requestRead: Boolean, requestWrite: Boolean): Boolean
    suspend fun requestSmbShares(input: SmbConnectionInput)
    suspend fun addSmbSource(input: SmbConnectionInput)
    suspend fun reenterSmbCredential(input: SmbCredentialInput)
    suspend fun forgetSmbSource(sourceId: String)
    suspend fun performFilerOperation(request: FilerOperationRequest)
    suspend fun resolveCollision(
        operationId: String,
        resolution: CollisionResolution,
        applyToAll: Boolean,
    )
    suspend fun selectFilmstripItem(id: String)
    suspend fun selectSource(source: SourceKind)
    suspend fun refreshFiler()
    suspend fun requestMangaSpread(request: MangaSpreadRequest)
    suspend fun exportCurrent(request: ExportRequest, uriToken: String? = null)
}

interface MobileSettingsStore {
    val settings: StateFlow<MobileViewerSettings>
    suspend fun replace(settings: MobileViewerSettings)
}

class EmptyMobileViewerController : MobileViewerController {
    override val snapshot = MutableStateFlow(ViewerEngineSnapshot())
    override suspend fun dispatch(action: ViewerAction) = Unit
    override suspend fun selectFilerEntry(id: String) = Unit
    override suspend fun navigateUp() = Unit
    override suspend fun navigateToBreadcrumb(id: String) = Unit
    override suspend fun addSafRoot(uriToken: String, requestRead: Boolean, requestWrite: Boolean) = false
    override suspend fun requestSmbShares(input: SmbConnectionInput) = Unit
    override suspend fun addSmbSource(input: SmbConnectionInput) = Unit
    override suspend fun reenterSmbCredential(input: SmbCredentialInput) = Unit
    override suspend fun forgetSmbSource(sourceId: String) = Unit
    override suspend fun performFilerOperation(request: FilerOperationRequest) = Unit
    override suspend fun resolveCollision(
        operationId: String,
        resolution: CollisionResolution,
        applyToAll: Boolean,
    ) = Unit
    override suspend fun selectFilmstripItem(id: String) = Unit
    override suspend fun selectSource(source: SourceKind) = Unit
    override suspend fun refreshFiler() = Unit
    override suspend fun requestMangaSpread(request: MangaSpreadRequest) = Unit
    override suspend fun exportCurrent(request: ExportRequest, uriToken: String?) = Unit
}

class InMemoryMobileSettingsStore(
    initial: MobileViewerSettings = MobileViewerSettings(),
) : MobileSettingsStore {
    override val settings = MutableStateFlow(initial)
    override suspend fun replace(settings: MobileViewerSettings) {
        this.settings.value = settings
    }
}
