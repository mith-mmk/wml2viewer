package io.github.mith_mmk.wml2viewer.data.controller

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.text.format.Formatter
import androidx.compose.ui.graphics.asAndroidBitmap
import io.github.mith_mmk.wml2viewer.data.config.MobileSourceProfileStore
import io.github.mith_mmk.wml2viewer.data.config.MobileLastLocation
import io.github.mith_mmk.wml2viewer.data.config.MobileLastLocationStore
import io.github.mith_mmk.wml2viewer.data.config.SavedSmbCredentialState
import io.github.mith_mmk.wml2viewer.data.config.SavedSmbSourceLifecycle
import io.github.mith_mmk.wml2viewer.data.config.proto.SafProfileV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SmbProfileV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.CreateRequest
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceCapabilities
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.data.source.SourceProvider
import io.github.mith_mmk.wml2viewer.data.source.TransferOperation
import io.github.mith_mmk.wml2viewer.data.source.WriteVerification
import io.github.mith_mmk.wml2viewer.data.transfer.TransferJobV1
import io.github.mith_mmk.wml2viewer.nativebridge.NativeReadingDirection
import io.github.mith_mmk.wml2viewer.nativebridge.NativeReadingLayout
import io.github.mith_mmk.wml2viewer.nativebridge.NativeReadingPage
import io.github.mith_mmk.wml2viewer.nativebridge.NativeReadingPlan
import io.github.mith_mmk.wml2viewer.nativebridge.NativeReadingPlanner
import io.github.mith_mmk.wml2viewer.nativebridge.NativeRequestError
import io.github.mith_mmk.wml2viewer.platform.AndroidPlatformGraph
import io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat as PlatformCodecFormat
import io.github.mith_mmk.wml2viewer.platform.codec.CodecRoute
import io.github.mith_mmk.wml2viewer.platform.codec.CodecRoutePolicy
import io.github.mith_mmk.wml2viewer.platform.codec.CodecLimitException
import io.github.mith_mmk.wml2viewer.platform.codec.NativeCodecException
import io.github.mith_mmk.wml2viewer.platform.codec.OsEncodeFormat
import io.github.mith_mmk.wml2viewer.platform.saf.SafSourceProvider
import io.github.mith_mmk.wml2viewer.platform.saf.newlyPersistedSafGrantModes
import io.github.mith_mmk.wml2viewer.platform.saf.orphanedPersistedSafGrantUris
import io.github.mith_mmk.wml2viewer.platform.security.CredentialInvalidatedException
import io.github.mith_mmk.wml2viewer.platform.smb.ShareDiscoveryResult
import io.github.mith_mmk.wml2viewer.platform.smb.SmbAuthenticationMode
import io.github.mith_mmk.wml2viewer.platform.smb.SmbProfile
import io.github.mith_mmk.wml2viewer.platform.smb.SmbSecurityStatus
import io.github.mith_mmk.wml2viewer.platform.smb.SmbSourceProvider
import io.github.mith_mmk.wml2viewer.platform.smb.SrvsvcShareEnumerationService
import io.github.mith_mmk.wml2viewer.ui.model.BreadcrumbUi
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.FilerCapabilitiesUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationRequest
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationType
import io.github.mith_mmk.wml2viewer.ui.model.FilerSortOrder
import io.github.mith_mmk.wml2viewer.ui.model.FilmstripItemUi
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.model.MangaLayoutMode
import io.github.mith_mmk.wml2viewer.ui.model.MangaPageRef
import io.github.mith_mmk.wml2viewer.ui.model.PendingCollisionUi
import io.github.mith_mmk.wml2viewer.ui.model.ReadingDirection
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbSecurityStatusUi
import io.github.mith_mmk.wml2viewer.ui.model.SourceKind
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.state.MangaSpreadRequest
import io.github.mith_mmk.wml2viewer.ui.state.MobileSettingsStore
import io.github.mith_mmk.wml2viewer.ui.state.MobileViewerController
import io.github.mith_mmk.wml2viewer.ui.state.NativeUiErrorMapper
import io.github.mith_mmk.wml2viewer.ui.state.UiError
import io.github.mith_mmk.wml2viewer.ui.state.UiErrorCode
import io.github.mith_mmk.wml2viewer.ui.state.ViewerEngineSnapshot
import io.github.mith_mmk.wml2viewer.ui.state.ViewerPageFrameUi
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Production adapter joining Compose state to opaque SAF/SMB providers and the Rust JNI core. */
class AndroidMobileViewerController(
    context: Context,
    private val graph: AndroidPlatformGraph,
    private val profileStore: MobileSourceProfileStore,
    private val locationStore: MobileLastLocationStore,
    private val settingsStore: MobileSettingsStore,
    private val scope: CoroutineScope,
) : MobileViewerController, AutoCloseable {
    private val applicationContext = context.applicationContext
    private val mutableSnapshot = MutableStateFlow(ViewerEngineSnapshot(loading = true))
    override val snapshot: StateFlow<ViewerEngineSnapshot> = mutableSnapshot.asStateFlow()
    private val pageLoader = ViewerPageLoader(graph, settingsStore)
    private val savedSmbSources = SavedSmbSourceLifecycle(profileStore, graph.credentialStore)
    private val sources = LinkedHashMap<String, RegisteredSource>()
    private val currentEntries = LinkedHashMap<String, SourceEntry>()
    private val navigation = ArrayList<NavigationNode>()
    private val pages = ArrayList<ViewerPageSource>()
    private val orientations = HashMap<String, Boolean>()
    private val pendingOperations = HashMap<String, PendingOperation>()
    private val pendingExportOperations = HashMap<String, PendingExportOperation>()
    private val generation = AtomicLong()
    private var sourceKind = SourceKind.LOCAL
    private var currentDirectory: EntryRef? = null
    private var currentPageIndex = 0
    private var openedEntry: EntryRef? = null
    private var openedArchive = false
    private var lastSpreadRequest: MangaSpreadRequest? = null
    private var currentLoadedPage: LoadedViewerPage? = null
    private var smbStatusJob: Job? = null
    private var animationJob: Job? = null
    private var prefetchJob: Job? = null
    @Volatile
    private var animationEnabled = true
    private var lastNonSingleMangaLayout = MangaLayoutMode.AUTO

    init {
        scope.launch { restoreSources() }
        scope.launch {
            val measured = runCatching {
                withContext(Dispatchers.Default) {
                    graph.codecRouter.capabilities.decodableFormats.mapNotNullTo(linkedSetOf()) {
                        it.toUiCodecFormat()
                    }
                }
            }.getOrDefault(emptySet())
            mutableSnapshot.update { it.copy(measuredOsCodecFormats = measured) }
        }
        scope.launch {
            var previousFilerSettings = settingsStore.settings.value.filer
            settingsStore.settings.collectLatest { settings ->
                val rememberLocationChanged =
                    previousFilerSettings.rememberLastLocation != settings.filer.rememberLastLocation
                val filerListingChanged = previousFilerSettings.showHiddenFiles != settings.filer.showHiddenFiles ||
                    previousFilerSettings.sortOrder != settings.filer.sortOrder
                previousFilerSettings = settings.filer
                if (settings.manga.layoutMode != MangaLayoutMode.SINGLE) {
                    lastNonSingleMangaLayout = settings.manga.layoutMode
                }
                val codecPolicy = settings.codecs.toPlatformPolicy()
                graph.codecRouter.updatePolicy(codecPolicy)
                val supportedExports = withContext(Dispatchers.Default) {
                    graph.codecRouter.availableEncodeFormats(codecPolicy).mapNotNullTo(linkedSetOf()) {
                        it.toUiExportFormat()
                    }
                }
                mutableSnapshot.update { it.copy(supportedExportFormats = supportedExports) }
                if (rememberLocationChanged) {
                    if (settings.filer.rememberLastLocation) persistLastLocation()
                    else withContext(Dispatchers.IO) { locationStore.replace(null) }
                }
                try {
                    updateCacheLimit(settings.cache.automaticLimit, settings.cache.manualLimitMiB)
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    mutableSnapshot.update { it.copy(error = error.toUiError()) }
                }
                if (filerListingChanged && currentDirectory != null) {
                    try {
                        refreshDirectory()
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        mutableSnapshot.update { it.copy(error = error.toUiError()) }
                    }
                }
            }
        }
    }

    override suspend fun dispatch(action: ViewerAction) {
        when (action) {
            ViewerAction.PREVIOUS_IMAGE -> moveLogicalPage(previous = true)
            ViewerAction.NEXT_IMAGE -> moveLogicalPage(previous = false)
            ViewerAction.FIRST_IMAGE -> showLogicalPage(0)
            ViewerAction.LAST_IMAGE -> showLogicalPage(pages.lastIndex)
            ViewerAction.RELOAD -> {
                animationJob?.cancel()
                cancelPrefetch()
                pageLoader.clearFrames()
                showLogicalPage(currentPageIndex)
            }
            ViewerAction.TOGGLE_MANGA_MODE -> {
                val current = settingsStore.settings.value
                val layout = if (current.manga.layoutMode == MangaLayoutMode.SINGLE) {
                    lastNonSingleMangaLayout
                } else {
                    lastNonSingleMangaLayout = current.manga.layoutMode
                    MangaLayoutMode.SINGLE
                }
                settingsStore.replace(current.copy(manga = current.manga.copy(layoutMode = layout)))
            }
            ViewerAction.TOGGLE_ANIMATION -> {
                animationEnabled = !animationEnabled
                animationJob?.cancel()
                showLogicalPage(currentPageIndex)
            }
            ViewerAction.TOGGLE_GRAYSCALE,
            ViewerAction.ZOOM_IN,
            ViewerAction.ZOOM_OUT,
            ViewerAction.ZOOM_RESET,
            ViewerAction.TOGGLE_FIT_MODE,
            ViewerAction.OPEN_FILER,
            ViewerAction.OPEN_SETTINGS,
            ViewerAction.OPEN_SUBFILER,
            ViewerAction.OPEN_CONTEXT_MENU,
            ViewerAction.EXPORT,
            ViewerAction.NONE,
            -> Unit
        }
    }

    override suspend fun selectFilerEntry(id: String) = guarded {
        val entry = currentEntries[id] ?: throw SourceException(
            SourceErrorCode.INVALID_REFERENCE,
            "The selected entry is no longer available",
        )
        if (entry.kind == EntryKind.DIRECTORY) {
            enterDirectory(entry)
            return@guarded
        }
        when {
            MobileFileTypes.archiveFormat(entry.name) != null -> openArchive(entry)
            MobileFileTypes.isImage(entry.name, entry.mimeType) -> openDirectoryImage(entry)
            else -> throw SourceException(SourceErrorCode.UNSUPPORTED, "The selected file is unsupported")
        }
    }

    override suspend fun navigateUp() = guarded {
        if (navigation.size > 1) {
            navigation.removeAt(navigation.lastIndex)
            currentDirectory = navigation.last().ref
            openedEntry = null
            openedArchive = false
            refreshDirectory()
            persistLastLocation()
        } else {
            navigation.clear()
            currentDirectory = null
            showSourceRoots()
            clearLastLocation()
        }
    }

    override suspend fun navigateToBreadcrumb(id: String) = guarded {
        val index = navigation.indexOfFirst { EntryUiTokenCodec.encode(it.ref) == id }
        if (index < 0) throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Breadcrumb is stale")
        while (navigation.lastIndex > index) navigation.removeAt(navigation.lastIndex)
        currentDirectory = navigation.last().ref
        openedEntry = null
        openedArchive = false
        refreshDirectory()
        persistLastLocation()
    }

    override suspend fun addSafRoot(
        uriToken: String,
        requestRead: Boolean,
        requestWrite: Boolean,
    ): Boolean {
        val uri = Uri.parse(uriToken)
        require(uri.scheme == "content" && DocumentsContract.isTreeUri(uri)) { "Invalid SAF tree" }
        require(requestRead) { "A SAF source requires read permission" }
        val resolver = applicationContext.contentResolver
        val existing = resolver.persistedUriPermissions.firstOrNull { it.uri == uri }
        val grantDelta = newlyPersistedSafGrantModes(
            requestedRead = requestRead,
            requestedWrite = requestWrite,
            existingRead = existing?.isReadPermission == true,
            existingWrite = existing?.isWritePermission == true,
        )
        val takeFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION or
            (if (requestWrite) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
        var grantPersisted = false
        var registrationSucceeded = false
        try {
            withContext(Dispatchers.IO) {
                resolver.takePersistableUriPermission(uri, takeFlags)
            }
            grantPersisted = true
            registrationSucceeded = guardedOutcome {
                val provider = graph.registerSaf(uri)
                var profilePersisted = false
                try {
                    val root = provider.stat(provider.root)
                    val registered = RegisteredSource(
                        sourceId = provider.providerId,
                        displayName = root.name,
                        kind = SourceKind.LOCAL,
                        provider = provider,
                    )
                    sources[provider.providerId] = registered
                    profileStore.upsert(
                        SourceProfileV1.newBuilder()
                            .setSourceId(registered.sourceId)
                            .setDisplayName(registered.displayName)
                            .setSaf(SafProfileV1.newBuilder().setTreeUri(uri.toString()))
                            .build(),
                    )
                    profilePersisted = true
                    sourceKind = SourceKind.LOCAL
                    enterSourceRoot(registered)
                } catch (error: Throwable) {
                    sources.remove(provider.providerId)
                    graph.unregister(provider.providerId)
                    if (profilePersisted) withContext(NonCancellable + Dispatchers.IO) {
                        runCatching { profileStore.remove(provider.providerId) }
                            .onFailure(error::addSuppressed)
                    }
                    throw error
                }
            }
            return registrationSucceeded
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            mutableSnapshot.update { it.copy(loading = false, error = error.toUiError()) }
            return false
        } finally {
            if (grantPersisted && !registrationSucceeded && (grantDelta.read || grantDelta.write)) {
                withContext(NonCancellable + Dispatchers.IO) {
                    val releaseFlags = (if (grantDelta.read) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                        (if (grantDelta.write) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
                    runCatching { resolver.releasePersistableUriPermission(uri, releaseFlags) }
                }
            }
        }
    }

    override suspend fun requestSmbShares(input: SmbConnectionInput) {
        val setupId = input.setupId ?: UUID.randomUUID().toString()
        mutableSnapshot.update {
            it.copy(smbSharesLoading = true, smbSetupId = setupId, availableSmbShares = emptyList(), error = null)
        }
        try {
            val profile = input.toSmbProfile(setupId, share = null)
            if (!input.guest) withContext(Dispatchers.IO) {
                require(input.password.isNotEmpty()) { "SMB password is required" }
                graph.credentialStore.put(setupId, input.password)
            }
            val result = SrvsvcShareEnumerationService(graph.credentialStore).enumerate(profile)
            val shares = when (result) {
                is ShareDiscoveryResult.Shares -> result.names
                is ShareDiscoveryResult.ManualShareRequired -> emptyList()
            }
            mutableSnapshot.update {
                it.copy(
                    availableSmbShares = shares,
                    smbSharesLoading = false,
                    smbSetupId = setupId,
                )
            }
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            mutableSnapshot.update { it.copy(smbSharesLoading = false, error = error.toUiError()) }
        } finally {
            if (!input.guest) withContext(NonCancellable + Dispatchers.IO) {
                runCatching { graph.credentialStore.delete(setupId) }
            }
        }
    }

    override suspend fun addSmbSource(input: SmbConnectionInput) = guarded {
        require(input.share.isNotBlank()) { "SMB share is required" }
        val profileId = input.setupId ?: UUID.randomUUID().toString()
        require(profileStore.current().none { it.sourceId == profileId }) { "SMB source already exists" }
        val profile = input.toSmbProfile(profileId, input.share)
        var provider: SmbSourceProvider? = null
        var profilePersisted = false
        var credentialAttempted = false
        try {
            if (!input.guest) withContext(Dispatchers.IO) {
                require(input.password.isNotEmpty()) { "SMB password is required" }
                credentialAttempted = true
                graph.credentialStore.put(profileId, input.password)
            }
            val registeredProvider = graph.registerSmb(profile)
            provider = registeredProvider
            registeredProvider.list(registeredProvider.root)
            val displayName = "${profile.server}/${profile.share}"
            val registered = RegisteredSource(profileId, displayName, SourceKind.SMB, registeredProvider)
            sources[registeredProvider.providerId] = registered
            profileStore.upsert(
                SourceProfileV1.newBuilder()
                    .setSourceId(profileId)
                    .setDisplayName(displayName)
                    .setSmb(
                        SmbProfileV1.newBuilder()
                            .setServer(profile.server)
                            .setPort(profile.port)
                            .setShare(profile.share.orEmpty())
                            .setUsername(profile.username.orEmpty())
                            .setDomain(profile.domain.orEmpty())
                            .setGuest(profile.authenticationMode == SmbAuthenticationMode.GUEST)
                            .setRequireEncryption(profile.requireEncryption)
                            .setCredentialId(
                                if (input.guest) "" else graph.credentialStore.credentialId(profileId),
                            ),
                    )
                    .build(),
            )
            profilePersisted = true
            sourceKind = SourceKind.SMB
            enterSourceRoot(registered)
            observeSmbSecurity(registeredProvider)
        } catch (error: Throwable) {
            provider?.let { registeredProvider ->
                sources.remove(registeredProvider.providerId)
                graph.unregister(registeredProvider.providerId)
            }
            withContext(NonCancellable + Dispatchers.IO) {
                if (profilePersisted) {
                    runCatching { profileStore.remove(profileId) }
                        .onFailure(error::addSuppressed)
                }
                if (credentialAttempted) {
                    runCatching { graph.credentialStore.delete(profileId) }
                        .onFailure(error::addSuppressed)
                }
            }
            throw error
        }
    }

    override suspend fun reenterSmbCredential(input: SmbCredentialInput) = guarded {
        val previous = sources.values.firstOrNull { it.sourceId == input.sourceId && it.kind == SourceKind.SMB }
        val saved = withContext(Dispatchers.IO) {
            savedSmbSources.replaceCredential(input.sourceId, input.password)
        }
        val nativeProfile = saved.toSmbProfile()
        smbStatusJob?.cancel()
        mutableSnapshot.update { it.copy(smbSecurityStatus = null) }
        previous?.let { registered ->
            sources.remove(registered.provider.providerId)
            graph.unregister(registered.provider.providerId)
        }
        val provider = graph.registerSmb(nativeProfile)
        try {
            provider.list(provider.root)
            sources[provider.providerId] = RegisteredSource(
                sourceId = saved.sourceId,
                displayName = saved.displayName.ifBlank { "${saved.smb.server}/${saved.smb.share}" },
                kind = SourceKind.SMB,
                provider = provider,
                credentialReentryRequired = false,
            )
        } catch (error: Throwable) {
            graph.unregister(provider.providerId)
            withContext(NonCancellable + Dispatchers.IO) {
                runCatching { graph.credentialStore.delete(saved.sourceId) }
            }
            val placeholder = graph.registerSmb(nativeProfile)
            sources[placeholder.providerId] = RegisteredSource(
                sourceId = saved.sourceId,
                displayName = saved.displayName.ifBlank { "${saved.smb.server}/${saved.smb.share}" },
                kind = SourceKind.SMB,
                provider = placeholder,
                credentialReentryRequired = true,
            )
            navigation.clear()
            currentDirectory = null
            sourceKind = SourceKind.SMB
            showSourceRoots()
            clearLastLocation()
            throw error
        }
        navigation.clear()
        currentDirectory = null
        sourceKind = SourceKind.SMB
        showSourceRoots()
        clearLastLocation()
    }

    override suspend fun forgetSmbSource(sourceId: String) = guarded {
        val registered = sources.values.firstOrNull { it.sourceId == sourceId && it.kind == SourceKind.SMB }
        registered?.let {
            sources.remove(it.provider.providerId)
            graph.unregister(it.provider.providerId)
        }
        try {
            withContext(NonCancellable + Dispatchers.IO) { savedSmbSources.forget(sourceId) }
        } finally {
            if (registered == null || currentDirectory?.providerId == registered.provider.providerId) {
                navigation.clear()
                currentDirectory = null
            }
            smbStatusJob?.cancel()
            mutableSnapshot.update { it.copy(smbSecurityStatus = null) }
            showSourceRoots()
            clearLastLocation()
        }
    }

    override suspend fun performFilerOperation(request: FilerOperationRequest) = guarded {
        executeOperation(PendingOperation(UUID.randomUUID().toString(), request), CollisionPolicy.FAIL)
    }

    override suspend fun resolveCollision(
        operationId: String,
        resolution: CollisionResolution,
        applyToAll: Boolean,
    ) = guarded {
        val policy = resolution.toCollisionPolicy()
        pendingExportOperations.remove(operationId)?.let { operation ->
            mutableSnapshot.update { it.copy(pendingCollision = null) }
            if (operation.systemTreeUri != null && policy == CollisionPolicy.REPLACE) {
                throw SourceException(
                    SourceErrorCode.UNSUPPORTED,
                    "Replacing an existing external-tree export is not crash-safe",
                )
            }
            if (policy != CollisionPolicy.SKIP) executePendingExport(operation, policy)
            return@guarded
        }
        val operation = pendingOperations.remove(operationId)
            ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Collision request expired")
        mutableSnapshot.update { it.copy(pendingCollision = null) }
        executeOperation(operation, policy)
    }

    override suspend fun selectFilmstripItem(id: String) {
        val index = pages.indexOfFirst { it.id == id }
        if (index >= 0) showLogicalPage(index)
    }

    override suspend fun selectSource(source: SourceKind) = guarded {
        sourceKind = source
        navigation.clear()
        currentDirectory = null
        openedEntry = null
        openedArchive = false
        smbStatusJob?.cancel()
        mutableSnapshot.update { it.copy(smbSecurityStatus = null) }
        showSourceRoots()
        clearLastLocation()
    }

    override suspend fun refreshFiler() = guarded {
        if (currentDirectory == null) showSourceRoots() else refreshDirectory()
    }

    override suspend fun exportCurrent(request: ExportRequest, uriToken: String?) = guarded {
        require(
            when (request.destination) {
                ExportDestination.CURRENT_DIRECTORY -> uriToken == null
                ExportDestination.SYSTEM_PICKER -> uriToken != null
            },
        ) { "Export destination does not match the selected output" }
        require(request.format in mutableSnapshot.value.supportedExportFormats) {
            "The selected export codec is unavailable"
        }
        require(request.quality in 0..100) { "Export quality is out of range" }
        val loaded = currentLoadedPage
            ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No page is available to export")
        val destinationName = normalizedExportName(request.fileName, request.format)
        val pendingExport = when (request.destination) {
            ExportDestination.CURRENT_DIRECTORY -> {
                val parent = currentDirectory
                    ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No export directory is selected")
                val provider = graph.sourceRegistry.require(parent)
                buildPendingExport(request, loaded, destinationName, parent, null, provider)
            }
            ExportDestination.SYSTEM_PICKER -> {
                val treeUriToken = checkNotNull(uriToken)
                val provider = SafSourceProvider(applicationContext, Uri.parse(treeUriToken))
                buildPendingExport(
                    request,
                    loaded,
                    destinationName,
                    provider.root,
                    treeUriToken,
                    provider,
                )
            }
        } ?: return@guarded
        mutableSnapshot.update { it.copy(exporting = true, error = null) }
        try {
            val encoded = encodeExport(loaded, request)
            try {
                exportToPendingDestination(pendingExport, encoded, CollisionPolicy.FAIL)
            } catch (error: SourceException) {
                if (error.code != SourceErrorCode.ALREADY_EXISTS) throw error
                queuePendingExport(pendingExport)
            }
        } finally {
            mutableSnapshot.update { it.copy(exporting = false) }
        }
    }

    private suspend fun buildPendingExport(
        request: ExportRequest,
        loaded: LoadedViewerPage,
        destinationName: String,
        parent: EntryRef,
        systemTreeUri: String?,
        provider: SourceProvider,
    ): PendingExportOperation? {
        val parentEntry = provider.stat(parent)
        if (!parentEntry.effectiveCapabilities.canCreate) {
            throw SourceException(SourceErrorCode.ACCESS_DENIED, "The current directory is read-only")
        }
        val operation = PendingExportOperation(
            id = UUID.randomUUID().toString(),
            request = request,
            pageId = loaded.page.id,
            parent = parent,
            destinationName = destinationName,
            systemTreeUri = systemTreeUri,
        )
        if (exportNameCollides(destinationName, provider.list(parent).map(SourceEntry::name))) {
            queuePendingExport(operation)
            return null
        }
        return operation
    }

    override suspend fun requestMangaSpread(request: MangaSpreadRequest) {
        if (pages.isEmpty()) return
        animationJob?.cancel()
        cancelPrefetch()
        pageLoader.cancelCurrent()
        val requestGeneration = generation.incrementAndGet()
        lastSpreadRequest = request
        var plan = buildReadingPlan(
            currentIndex = request.currentLogicalPageIndex,
            landscape = request.landscape,
            prefetchSpreads = request.prefetchSpreads,
        )
        mutableSnapshot.update { it.copy(loading = true, error = null) }
        try {
            val currentPageId = pages.getOrNull(request.currentLogicalPageIndex)?.id
                ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Logical page is unavailable")
            var requestedPages = plan.visualIndices.map(pages::get)
            val loadedById = requestedPages
                .sortedByDescending { it.id == currentPageId }
                .associate { page -> page.id to pageLoader.load(page) }
                .toMutableMap()
            loadedById.values.forEach { orientations[it.page.id] = it.portrait }

            // Unknown orientations optimistically start as portrait. Re-plan in Rust
            // after the first decode so a landscape companion is never displayed.
            plan = buildReadingPlan(
                currentIndex = request.currentLogicalPageIndex,
                landscape = request.landscape,
                prefetchSpreads = request.prefetchSpreads,
            )
            requestedPages = plan.visualIndices.map(pages::get)
            requestedPages.forEach { page ->
                if (page.id !in loadedById) {
                    pageLoader.load(page).also { loaded ->
                        orientations[loaded.page.id] = loaded.portrait
                        loadedById[page.id] = loaded
                    }
                }
            }
            val loaded = requestedPages.map { loadedById.getValue(it.id) }
            if (requestGeneration != generation.get()) return
            val pageRefs = pageRefs()
            val frames = loaded.map { item ->
                ViewerPageFrameUi(pageRefs.first { it.id == item.page.id }, item.frame)
            }
            val currentFrame = loaded.firstOrNull { it.page.id == pages[currentPageIndex].id }?.frame
                ?: loaded.first().frame
            mutableSnapshot.update {
                it.copy(
                    frame = currentFrame,
                    mangaPages = pageRefs,
                    spreadFrames = frames,
                    renderedViewportGeneration = request.viewportGeneration,
                    loading = false,
                    error = null,
                )
            }
            val animated = loaded.firstOrNull { it.page.id == pages[currentPageIndex].id }
            if (animated != null) {
                currentLoadedPage = animated
                startAnimation(
                    loaded = animated,
                    pageRef = pageRefs.first { it.id == animated.page.id },
                    requestGeneration = requestGeneration,
                )
            }
            prefetch(plan.preloadIndices)
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            if (requestGeneration == generation.get()) {
                mutableSnapshot.update { it.copy(loading = false, error = error.toUiError()) }
            }
        }
    }

    private suspend fun restoreSources() {
        try {
            val profiles = profileStore.current()
            reconcilePersistedSafGrants(profiles)
            profiles.forEach { profile ->
                runCatching {
                    when (profile.sourceCase) {
                        SourceProfileV1.SourceCase.SAF -> restoreSaf(profile)
                        SourceProfileV1.SourceCase.SMB -> restoreSmb(profile)
                        else -> Unit
                    }
                }
            }
            graph.installTransferRuntime()
            graph.transferScheduler.recover { job ->
                job.sourceProviderId.startsWith("smb:") || job.destinationProviderId.startsWith("smb:")
            }
            val restoredLocation = runCatching { restoreLastLocation() }.getOrDefault(false)
            if (!restoredLocation) showSourceRoots()
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            mutableSnapshot.update { it.copy(loading = false, error = error.toUiError()) }
        }
    }

    private suspend fun reconcilePersistedSafGrants(profiles: List<SourceProfileV1>) {
        val ownedUris = profiles.asSequence()
            .filter { it.sourceCase == SourceProfileV1.SourceCase.SAF }
            .map { it.saf.treeUri }
            .filter(String::isNotBlank)
            .toHashSet()
        withContext(Dispatchers.IO) {
            val resolver = applicationContext.contentResolver
            val orphaned = orphanedPersistedSafGrantUris(
                profileUris = ownedUris,
                persistedUris = resolver.persistedUriPermissions
                    .mapTo(hashSetOf()) { it.uri.toString() },
            )
            resolver.persistedUriPermissions
                .filter { it.uri.toString() in orphaned }
                .forEach { permission ->
                    val flags = (if (permission.isReadPermission) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                        (if (permission.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
                    if (flags != 0) runCatching {
                        resolver.releasePersistableUriPermission(permission.uri, flags)
                    }
                }
        }
    }

    private suspend fun restoreLastLocation(): Boolean {
        val saved = withContext(Dispatchers.IO) { locationStore.currentIfRemembered() } ?: return false
        val source = sources.values.firstOrNull { it.sourceId == saved.sourceId } ?: return false
        val provider = source.provider
        val directory = EntryRef(provider.providerId, saved.directoryOpaqueEntryId)
        val restoredNavigation = restoreNavigation(provider, source, directory) ?: return false

        sourceKind = source.kind
        navigation.clear()
        navigation += restoredNavigation
        currentDirectory = directory
        openedEntry = null
        openedArchive = false
        refreshDirectory()
        if (provider is SmbSourceProvider) observeSmbSecurity(provider)

        val openedOpaqueId = saved.openedEntryOpaqueEntryId ?: return true
        val selected = provider.stat(EntryRef(provider.providerId, openedOpaqueId))
        if (selected.kind != EntryKind.FILE) return true
        if (saved.openedArchive) {
            val format = MobileFileTypes.archiveFormat(selected.name) ?: return true
            if (format.isBlank()) return true
            openArchive(selected, saved.logicalPageIndex)
        } else if (MobileFileTypes.isImage(selected.name, selected.mimeType)) {
            openDirectoryImage(selected)
        }
        return true
    }

    private suspend fun restoreNavigation(
        provider: SourceProvider,
        source: RegisteredSource,
        directory: EntryRef,
    ): List<NavigationNode>? {
        val reversed = ArrayList<NavigationNode>()
        var current = provider.stat(directory)
        repeat(MAX_RESTORED_NAVIGATION_DEPTH) {
            reversed += NavigationNode(
                current.ref,
                if (current.ref == provider.root) source.displayName else current.name,
            )
            if (current.ref == provider.root) return reversed.asReversed()
            val parent = current.parent ?: return null
            current = provider.stat(parent)
        }
        return null
    }

    private suspend fun persistLastLocation() {
        val settings = settingsStore.settings.value.filer
        if (!settings.rememberLastLocation) return
        val directory = currentDirectory ?: return clearLastLocation()
        val source = sources[directory.providerId] ?: return
        val currentPage = pages.getOrNull(currentPageIndex)
        val persistedOpenedEntry = if (openedArchive) {
            openedEntry
        } else {
            (currentPage as? ViewerPageSource.Direct)?.entry?.ref ?: openedEntry
        }
        val location = MobileLastLocation(
            sourceId = source.sourceId,
            directoryOpaqueEntryId = directory.opaqueId,
            openedEntryOpaqueEntryId = persistedOpenedEntry?.opaqueId,
            logicalPageIndex = if (openedArchive) currentPageIndex else 0,
            openedArchive = openedArchive,
        )
        runCatching { withContext(Dispatchers.IO) { locationStore.replace(location) } }
    }

    private suspend fun clearLastLocation() {
        runCatching { withContext(Dispatchers.IO) { locationStore.replace(null) } }
    }

    private fun restoreSaf(profile: SourceProfileV1) {
        val uri = Uri.parse(profile.saf.treeUri)
        val permission = applicationContext.contentResolver.persistedUriPermissions.firstOrNull {
            it.uri == uri && it.isReadPermission
        } ?: return
        val provider = graph.registerSaf(permission.uri)
        sources[provider.providerId] = RegisteredSource(
            profile.sourceId,
            profile.displayName.ifBlank { permission.uri.authority.orEmpty() },
            SourceKind.LOCAL,
            provider,
        )
    }

    private suspend fun restoreSmb(profile: SourceProfileV1) {
        val smb = profile.smb
        val credentialState = withContext(Dispatchers.IO) { savedSmbSources.credentialState(profile) }
        val nativeProfile = profile.toSmbProfile()
        val provider = graph.registerSmb(nativeProfile)
        sources[provider.providerId] = RegisteredSource(
            profile.sourceId,
            profile.displayName.ifBlank { "${smb.server}/${smb.share}" },
            SourceKind.SMB,
            provider,
            credentialReentryRequired = credentialState == SavedSmbCredentialState.REENTRY_REQUIRED,
        )
    }

    private suspend fun enterSourceRoot(source: RegisteredSource) {
        navigation.clear()
        val root = source.provider.stat(source.provider.root)
        navigation += NavigationNode(root.ref, source.displayName)
        currentDirectory = root.ref
        openedEntry = null
        openedArchive = false
        refreshDirectory()
        if (source.provider is SmbSourceProvider) observeSmbSecurity(source.provider)
        persistLastLocation()
    }

    private suspend fun enterDirectory(entry: SourceEntry) {
        val source = sources[entry.ref.providerId]
        if (source != null && entry.ref == source.provider.root) {
            enterSourceRoot(source)
            return
        }
        val existing = navigation.indexOfFirst { it.ref == entry.ref }
        if (existing >= 0) {
            while (navigation.lastIndex > existing) navigation.removeAt(navigation.lastIndex)
        } else {
            navigation += NavigationNode(entry.ref, entry.name)
        }
        currentDirectory = entry.ref
        openedEntry = null
        openedArchive = false
        refreshDirectory()
        persistLastLocation()
    }

    private fun showSourceRoots() {
        openedEntry = null
        openedArchive = false
        currentEntries.clear()
        val entries = sources.values.filter { it.kind == sourceKind }.map { source ->
            val entry = SourceEntry(
                ref = source.provider.root,
                parent = null,
                name = source.displayName,
                kind = EntryKind.DIRECTORY,
                mimeType = null,
                size = null,
                modifiedAtEpochMillis = null,
                effectiveCapabilities = sourceRootRowCapabilities(),
            )
            Triple(EntryUiTokenCodec.encode(entry.ref), entry, source)
        }
        currentEntries.putAll(entries.associate { (id, entry, _) -> id to entry })
        mutableSnapshot.update {
            it.copy(
                filerEntries = entries.map { (id, entry, source) ->
                    entry.toUi(id).copy(
                        managedSourceId = source.sourceId.takeIf { source.kind == SourceKind.SMB },
                        credentialReentryRequired = source.credentialReentryRequired,
                        canForgetSource = source.kind == SourceKind.SMB,
                    )
                },
                sourceKind = sourceKind,
                pathLabel = "",
                currentDirectoryId = null,
                breadcrumb = emptyList(),
                atSourceRoot = true,
                currentCapabilities = FilerCapabilitiesUi(),
                loading = false,
                error = null,
            )
        }
    }

    private suspend fun refreshDirectory() {
        val directory = currentDirectory ?: return showSourceRoots()
        val provider = graph.sourceRegistry.require(directory)
        val filerSettings = settingsStore.settings.value.filer
        val listed = provider.list(directory)
            .asSequence()
            .filter { filerSettings.showHiddenFiles || !it.isHidden }
            .sortedWith(entryComparator(filerSettings.sortOrder))
            .toList()
        currentEntries.clear()
        listed.forEach { currentEntries[EntryUiTokenCodec.encode(it.ref)] = it }
        val currentStat = provider.stat(directory)
        val root = sources[directory.providerId]?.provider?.root
        mutableSnapshot.update {
            it.copy(
                filerEntries = currentEntries.map { (id, entry) -> entry.toUi(id) },
                sourceKind = sourceKind,
                pathLabel = navigation.joinToString(" / ") { node -> node.label },
                currentDirectoryId = EntryUiTokenCodec.encode(directory),
                breadcrumb = navigation.mapIndexed { index, node ->
                    BreadcrumbUi(EntryUiTokenCodec.encode(node.ref), node.label, index == 0)
                },
                atSourceRoot = directory == root,
                currentCapabilities = currentStat.effectiveCapabilities.toUi(),
                loading = false,
                error = null,
            )
        }
    }

    private suspend fun openDirectoryImage(selected: SourceEntry) {
        animationJob?.cancel()
        pageLoader.clearFrames()
        val ordered = currentEntries.values
            .filter { it.kind == EntryKind.FILE && MobileFileTypes.isImage(it.name, it.mimeType) }
            .sortedWith(compareBy(NaturalFileNameComparator) { it.name })
        openedEntry = selected.ref
        openedArchive = false
        setPages(ordered.map(ViewerPageSource::Direct), ordered.indexOfFirst { it.ref == selected.ref })
    }

    private suspend fun openArchive(selected: SourceEntry, selectedIndex: Int = 0) {
        cancelPrefetch()
        pageLoader.cancelCurrent()
        val format = MobileFileTypes.archiveFormat(selected.name)!!
        val archivePages = pageLoader.openArchive(selected, format)
        if (archivePages.isEmpty()) throw SourceException(SourceErrorCode.UNSUPPORTED, "Archive has no image entries")
        val resolvedPages: List<ViewerPageSource> = if (format == "wmltxt") {
            val base = selected.parent ?: currentDirectory
                ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Listed-file base is unavailable")
            val identity = EntryUiTokenCodec.encode(selected.ref)
            archivePages.map { page ->
                ViewerPageSource.ListedEntry(
                    baseDirectory = base,
                    relativePath = page.name,
                    listedIdentity = identity,
                    entryIndex = page.entryIndex,
                )
            }
        } else {
            archivePages
        }
        openedEntry = selected.ref
        openedArchive = true
        setPages(resolvedPages, selectedIndex)
    }

    private suspend fun setPages(newPages: List<ViewerPageSource>, selectedIndex: Int) {
        animationJob?.cancel()
        cancelPrefetch()
        if (newPages.none { it is ViewerPageSource.ArchiveEntry }) {
            pageLoader.leaveArchive()
        }
        generation.incrementAndGet()
        pages.clear()
        pages += newPages
        orientations.keys.retainAll(pages.mapTo(HashSet()) { it.id })
        currentPageIndex = selectedIndex.coerceIn(0, pages.lastIndex)
        currentLoadedPage = null
        lastSpreadRequest = null
        publishPageModel(loading = true)
        showLogicalPage(currentPageIndex)
    }

    private suspend fun showLogicalPage(index: Int) {
        if (index !in pages.indices) return
        animationJob?.cancel()
        cancelPrefetch()
        pageLoader.cancelCurrent()
        val requestGeneration = generation.incrementAndGet()
        currentPageIndex = index
        publishPageModel(loading = true)
        try {
            val loaded = pageLoader.load(pages[index])
            if (requestGeneration != generation.get()) return
            orientations[loaded.page.id] = loaded.portrait
            currentLoadedPage = loaded
            val ref = pageRefs()[index]
            mutableSnapshot.update {
                it.copy(
                    title = loaded.page.name,
                    frame = loaded.frame,
                    mangaPages = pageRefs(),
                    currentLogicalPageIndex = index,
                    spreadFrames = listOf(ViewerPageFrameUi(ref, loaded.frame)),
                    filmstrip = filmstrip(),
                    loading = false,
                    error = null,
                )
            }
            startAnimation(loaded, ref, requestGeneration)
            persistLastLocation()
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            if (requestGeneration == generation.get()) {
                mutableSnapshot.update { it.copy(loading = false, error = error.toUiError()) }
            }
        }
    }

    private suspend fun moveLogicalPage(previous: Boolean) {
        if (pages.isEmpty()) return
        val plan = buildReadingPlan(
            currentIndex = currentPageIndex,
            landscape = lastSpreadRequest?.landscape == true,
            prefetchSpreads = 0,
        )
        val next = if (previous) plan.previousAnchorIndex else plan.nextAnchorIndex
        if (next != null) showLogicalPage(next)
    }

    private fun publishPageModel(loading: Boolean) {
        mutableSnapshot.update {
            it.copy(
                title = pages.getOrNull(currentPageIndex)?.name.orEmpty(),
                mangaPages = pageRefs(),
                currentLogicalPageIndex = currentPageIndex,
                spreadFrames = emptyList(),
                filmstrip = filmstrip(),
                loading = loading,
                error = null,
            )
        }
    }

    private fun pageRefs(): List<MangaPageRef> = pages.map { page ->
        MangaPageRef(page.id, page.sourceBoundary, orientations[page.id] ?: true)
    }

    private fun filmstrip(): List<FilmstripItemUi> = pages.mapIndexed { index, page ->
        FilmstripItemUi(page.id, page.name, index == currentPageIndex)
    }

    private fun prefetch(indices: List<Int>) {
        cancelPrefetch()
        val requested = indices.distinct().filter { it in pages.indices }
        if (requested.isEmpty()) return
        val prefetchGeneration = generation.get()
        prefetchJob = scope.launch {
            requested.forEach { index ->
                val page = pages[index]
                try {
                    val portrait = pageLoader.prefetch(page)
                    if (prefetchGeneration == generation.get()) orientations[page.id] = portrait
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                }
            }
        }
    }

    private fun buildReadingPlan(
        currentIndex: Int,
        landscape: Boolean,
        prefetchSpreads: Int,
    ): NativeReadingPlan {
        if (pages.size > NativeReadingPlanner.MAX_PAGES) throw ReadingPlanLimitException()
        val sourceIds = LinkedHashMap<String, Long>()
        val coveredSources = HashSet<String>()
        val nativePages = pages.map { page ->
            NativeReadingPage(
                sourceId = sourceIds.getOrPut(page.sourceBoundary) { sourceIds.size.toLong() + 1L },
                portrait = orientations[page.id] ?: true,
                cover = coveredSources.add(page.sourceBoundary),
            )
        }
        val settings = settingsStore.settings.value.manga
        return NativeReadingPlanner.plan(
            pages = nativePages,
            currentIndex = currentIndex,
            isLandscape = landscape,
            layout = when (settings.layoutMode) {
                MangaLayoutMode.AUTO -> NativeReadingLayout.AUTO
                MangaLayoutMode.SINGLE -> NativeReadingLayout.SINGLE
                MangaLayoutMode.SPREAD -> NativeReadingLayout.SPREAD
            },
            direction = when (settings.readingDirection) {
                ReadingDirection.LEFT_TO_RIGHT -> NativeReadingDirection.LEFT_TO_RIGHT
                ReadingDirection.RIGHT_TO_LEFT -> NativeReadingDirection.RIGHT_TO_LEFT
            },
            coverAlone = settings.singleCover,
            maxPrefetchSpreads = prefetchSpreads,
        ) ?: throw SourceException(SourceErrorCode.IO, "Rust reading planner rejected the request")
    }

    private fun cancelPrefetch() {
        val running = prefetchJob
        if (running?.isActive == true) pageLoader.cancelCurrent()
        running?.cancel()
        prefetchJob = null
    }

    private fun startAnimation(
        loaded: LoadedViewerPage,
        pageRef: MangaPageRef,
        requestGeneration: Long,
    ) {
        animationJob?.cancel()
        val frameCount = loaded.animationFrameCount
        if (!animationEnabled || frameCount < 2) return
        val passes = AnimationPlaybackPolicy.playbackPasses(loaded.loopCount)
        animationJob = scope.launch {
            try {
                var pass = 0L
                while (isActive && generation.get() == requestGeneration && pass < passes) {
                    for (index in 0 until frameCount) {
                        if (!isActive || generation.get() != requestGeneration) return@launch
                        val animationFrame = if (loaded.animationSource == null) {
                            loaded.animationFrame(index)
                        } else {
                            withContext(Dispatchers.Default) { loaded.animationFrame(index) }
                        }
                        if (index != 0 || pass != 0L) {
                            mutableSnapshot.update { state ->
                                state.copy(
                                    frame = animationFrame.frame,
                                    spreadFrames = state.spreadFrames.map { item ->
                                        if (item.page.id == pageRef.id) {
                                            item.copy(frame = animationFrame.frame)
                                        } else {
                                            item
                                        }
                                    },
                                )
                            }
                        }
                        delay(animationFrame.durationMillis)
                    }
                    pass++
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                if (generation.get() == requestGeneration) {
                    mutableSnapshot.update { it.copy(error = error.toUiError()) }
                }
            }
        }
    }

    private suspend fun writeExport(
        provider: SourceProvider,
        parent: EntryRef,
        name: String,
        mimeType: String,
        encoded: ByteArray,
        collisionPolicy: CollisionPolicy,
    ) {
        val parentEntry = provider.stat(parent)
        if (!parentEntry.effectiveCapabilities.canCreate) {
            throw SourceException(SourceErrorCode.ACCESS_DENIED, "The current directory is read-only")
        }
        val session = provider.create(
            parent,
            CreateRequest(name, mimeType, collisionPolicy),
        )
        var committed = false
        try {
            session.output.use { output -> output.write(encoded) }
            val expected = WriteVerification(encoded.size.toLong(), encoded.sha256Hex())
            if (!session.verify(expected)) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Export verification failed")
            }
            session.commit()
            committed = true
            if (parent == currentDirectory) refreshDirectory()
        } finally {
            if (!committed) withContext(NonCancellable) {
                runCatching { session.abort() }
            }
        }
    }

    private suspend fun exportToPendingDestination(
        operation: PendingExportOperation,
        encoded: ByteArray,
        collisionPolicy: CollisionPolicy,
    ) {
        val provider = operation.systemTreeUri?.let { SafSourceProvider(applicationContext, Uri.parse(it)) }
            ?: graph.sourceRegistry.require(operation.parent)
        writeExport(
            provider = provider,
            parent = provider.root.takeIf { operation.systemTreeUri != null } ?: operation.parent,
            name = operation.destinationName,
            mimeType = operation.request.format.mimeType,
            encoded = encoded,
            collisionPolicy = collisionPolicy,
        )
    }

    private fun queuePendingExport(operation: PendingExportOperation) {
        pendingExportOperations.clear()
        pendingExportOperations[operation.id] = operation
        mutableSnapshot.update {
            it.copy(
                pendingCollision = PendingCollisionUi(
                    operationId = operation.id,
                    displayName = operation.destinationName,
                    allowReplace = operation.systemTreeUri == null,
                    supportsApplyToAll = false,
                ),
            )
        }
    }

    private suspend fun executePendingExport(operation: PendingExportOperation, policy: CollisionPolicy) {
        val loaded = currentLoadedPage
        if (loaded?.page?.id != operation.pageId) {
            throw SourceException(SourceErrorCode.INVALID_REFERENCE, "The export page changed while resolving the collision")
        }
        require(operation.request.format in mutableSnapshot.value.supportedExportFormats) {
            "The selected export codec is unavailable"
        }
        mutableSnapshot.update { it.copy(exporting = true, error = null) }
        try {
            val encoded = encodeExport(loaded, operation.request)
            exportToPendingDestination(operation, encoded, policy)
        } finally {
            mutableSnapshot.update { it.copy(exporting = false) }
        }
    }

    private suspend fun encodeExport(loaded: LoadedViewerPage, request: ExportRequest): ByteArray =
        withContext(Dispatchers.Default) {
            graph.codecRouter.encode(
                bitmap = loaded.frame.asAndroidBitmap(),
                format = request.format.toPlatformExportFormat(),
                quality = request.quality,
            ).also { encoded ->
                if (encoded.isEmpty() || encoded.size.toLong() > MAX_EXPORT_BYTES) {
                    throw SourceException(SourceErrorCode.INTEGRITY, "Encoded export exceeds the safe limit")
                }
            }
        }

    private suspend fun executeOperation(operation: PendingOperation, policy: CollisionPolicy) {
        val request = operation.request
        try {
            when (request.type) {
                FilerOperationType.CREATE_FOLDER -> {
                    val parent = request.destinationId?.let(EntryUiTokenCodec::decode)
                        ?: currentDirectory ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No destination")
                    if (currentDirectory == null) {
                        withNonRootSourceOperation(parent, registeredRoot(parent)) { Unit }
                    }
                    graph.sourceRegistry.require(parent)
                        .createDirectory(parent, request.name?.trim().orEmpty(), policy)
                    refreshDirectory()
                }
                FilerOperationType.RENAME -> {
                    val source = request.entryId?.let { requireEntry(it) }
                        ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No source")
                    withNonRootSourceOperation(source.ref, registeredRoot(source.ref)) {
                        graph.sourceRegistry.require(source.ref)
                            .rename(source.ref, request.name?.trim().orEmpty(), policy)
                    }
                    refreshDirectory()
                }
                FilerOperationType.DELETE -> {
                    val source = request.entryId?.let { requireEntry(it) }
                        ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No source")
                    withNonRootSourceOperation(source.ref, registeredRoot(source.ref)) {
                        graph.sourceRegistry.require(source.ref)
                            .trashOrDelete(source.ref, request.allowPermanentDelete)
                    }
                    refreshDirectory()
                }
                FilerOperationType.COPY, FilerOperationType.MOVE -> {
                    val source = request.entryId?.let { requireEntry(it) }
                        ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No source")
                    val destination = request.destinationId?.let(EntryUiTokenCodec::decode)
                        ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "No destination")
                    val destinationProvider = graph.sourceRegistry.require(destination)
                    val destinationEntry = destinationProvider.stat(destination)
                    if (destinationEntry.kind != EntryKind.DIRECTORY ||
                        !destinationEntry.effectiveCapabilities.canCreate
                    ) {
                        throw SourceException(SourceErrorCode.ACCESS_DENIED, "Destination directory is read-only")
                    }
                    withNonRootSourceOperation(source.ref, registeredRoot(source.ref)) {
                        val destinationName = request.name?.takeIf { it.isNotBlank() } ?: source.name
                        if (policy == CollisionPolicy.FAIL && destinationProvider
                                .list(destination).any { it.name == destinationName }
                        ) {
                            throw SourceException(SourceErrorCode.ALREADY_EXISTS, "Destination already exists")
                        }
                        val job = TransferJobV1.create(
                            source.ref,
                            destination,
                            destinationName,
                            if (request.type == FilerOperationType.MOVE) TransferOperation.MOVE else TransferOperation.COPY,
                            policy,
                        )
                        graph.transferScheduler.enqueue(
                            job,
                            source.ref.providerId.startsWith("smb:") || destination.providerId.startsWith("smb:"),
                        )
                    }
                }
            }
        } catch (error: SourceException) {
            if (error.code == SourceErrorCode.ALREADY_EXISTS && policy == CollisionPolicy.FAIL) {
                pendingOperations[operation.id] = operation
                val displayName = request.name?.takeIf { it.isNotBlank() }
                    ?: request.entryId?.let { id -> runCatching { requireEntry(id).name }.getOrNull() }
                    ?: ""
                mutableSnapshot.update {
                    it.copy(
                        pendingCollision = PendingCollisionUi(
                            operation.id,
                            displayName,
                        ),
                    )
                }
                return
            }
            throw error
        }
    }

    private suspend fun requireEntry(id: String): SourceEntry = currentEntries[id] ?: run {
        val ref = try {
            EntryUiTokenCodec.decode(id)
        } catch (error: IllegalArgumentException) {
            throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Entry is stale", error)
        }
        graph.sourceRegistry.require(ref).stat(ref)
    }

    private fun registeredRoot(ref: EntryRef): EntryRef? = sources[ref.providerId]?.provider?.root

    private fun observeSmbSecurity(provider: SmbSourceProvider) {
        smbStatusJob?.cancel()
        smbStatusJob = scope.launch {
            provider.securityStatus.collectLatest { status ->
                mutableSnapshot.update { it.copy(smbSecurityStatus = status.toUi()) }
            }
        }
    }

    private suspend fun updateCacheLimit(automatic: Boolean, manualMiB: Int) = withContext(Dispatchers.IO) {
        val usable = applicationContext.cacheDir.usableSpace
        val limit = if (automatic) MobileCacheLimitPolicy.automatic(usable)
        else MobileCacheLimitPolicy.manual(manualMiB, usable)
        graph.fileCache.updateLimits(
            limit.maxBytes,
            2_048,
            minOf(limit.maxBytes, MobileCacheLimitPolicy.MAX_SINGLE_ENTRY_BYTES),
        )
    }

    private suspend fun guarded(block: suspend () -> Unit) {
        guardedOutcome(block)
    }

    private suspend fun guardedOutcome(block: suspend () -> Unit): Boolean {
        mutableSnapshot.update { it.copy(error = null) }
        return try {
            block()
            true
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            if (error is CredentialInvalidatedException) markCredentialInvalidated(error)
            mutableSnapshot.update { it.copy(loading = false, error = error.toUiError()) }
            false
        }
    }

    private suspend fun markCredentialInvalidated(error: CredentialInvalidatedException) {
        val source = sources.values.firstOrNull {
            it.kind == SourceKind.SMB && graph.credentialStore.credentialId(it.sourceId) == error.credentialId
        } ?: return
        try {
            withContext(NonCancellable + Dispatchers.IO) {
                graph.credentialStore.discardInvalidated(source.sourceId, error)
            }
        } catch (cleanupError: Throwable) {
            error.addSuppressed(cleanupError)
        }
        source.credentialReentryRequired = true
        navigation.clear()
        currentDirectory = null
        sourceKind = SourceKind.SMB
        showSourceRoots()
        clearLastLocation()
    }

    override fun close() {
        generation.incrementAndGet()
        smbStatusJob?.cancel()
        animationJob?.cancel()
        cancelPrefetch()
        pendingExportOperations.clear()
        pageLoader.close()
    }

    private fun SourceEntry.toUi(id: String) = FilerEntryUi(
        id = id,
        name = name,
        isContainer = kind == EntryKind.DIRECTORY,
        subtitle = size?.let { Formatter.formatShortFileSize(applicationContext, it) },
        capabilities = effectiveCapabilities.toUi(),
    )

    private fun SourceCapabilities.toUi() = FilerCapabilitiesUi(
        canCreate = canCreate,
        canCopy = canRead || canTransferDirectoriesAcrossProviders,
        canMove = (canRead || canTransferDirectoriesAcrossProviders) && canDelete,
        canRename = canRename,
        canTrash = canTrash,
        canDeletePermanently = canDelete,
    )

    private fun SmbSecurityStatus.toUi() = if (!connected) null else SmbSecurityStatusUi(
        signingActive = signingActive,
        encryptionActive = encryptionActive,
        dialect = dialect,
    )

    private fun SmbConnectionInput.toSmbProfile(profileId: String, share: String?) = SmbProfile(
        profileId = profileId,
        server = server.trim(),
        port = port,
        share = share,
        username = username.trim().takeUnless { guest },
        domain = domain.trim().takeIf { it.isNotBlank() },
        authenticationMode = if (guest) SmbAuthenticationMode.GUEST else SmbAuthenticationMode.USER_PASSWORD,
        requireEncryption = requireEncryption,
    )

    private fun SourceProfileV1.toSmbProfile(): SmbProfile {
        require(sourceCase == SourceProfileV1.SourceCase.SMB) { "SMB profile is required" }
        return SmbProfile(
            profileId = sourceId,
            server = smb.server,
            port = smb.port.toInt(),
            share = smb.share,
            username = smb.username.takeUnless { smb.guest },
            domain = smb.domain.takeIf { it.isNotBlank() },
            authenticationMode = if (smb.guest) SmbAuthenticationMode.GUEST else SmbAuthenticationMode.USER_PASSWORD,
            requireEncryption = smb.requireEncryption,
        )
    }

    private fun io.github.mith_mmk.wml2viewer.ui.model.CodecSettings.toPlatformPolicy(): CodecRoutePolicy =
        CodecRoutePolicy(
            global = defaultPolicy.toPlatformRoute(default = CodecRoute.INTERNAL_FIRST),
            overrides = overrides.mapKeys { (format, _) -> format.toPlatformFormat() }
                .mapValues { (_, policy) -> policy.toPlatformRoute() },
        )

    private fun CodecFormat.toPlatformFormat() = PlatformCodecFormat.valueOf(name)

    private fun PlatformCodecFormat.toUiCodecFormat(): CodecFormat? =
        takeUnless { it == PlatformCodecFormat.UNKNOWN }?.let { CodecFormat.valueOf(it.name) }

    private fun CodecPolicy.toPlatformRoute(default: CodecRoute = CodecRoute.DEFAULT) = when (this) {
        CodecPolicy.DEFAULT -> default
        CodecPolicy.INTERNAL_FIRST -> CodecRoute.INTERNAL_FIRST
        CodecPolicy.OS_FIRST -> CodecRoute.OS_FIRST
        CodecPolicy.INTERNAL_ONLY -> CodecRoute.INTERNAL_ONLY
        CodecPolicy.OS_ONLY -> CodecRoute.OS_ONLY
    }

    private fun OsEncodeFormat.toUiExportFormat(): ExportFormat? = when (this) {
        OsEncodeFormat.PNG -> ExportFormat.PNG
        OsEncodeFormat.JPEG -> ExportFormat.JPEG
        OsEncodeFormat.WEBP_LOSSY -> ExportFormat.WEBP_LOSSY
        OsEncodeFormat.WEBP_LOSSLESS -> ExportFormat.WEBP_LOSSLESS
    }

    private fun ExportFormat.toPlatformExportFormat(): OsEncodeFormat = when (this) {
        ExportFormat.PNG -> OsEncodeFormat.PNG
        ExportFormat.JPEG -> OsEncodeFormat.JPEG
        ExportFormat.WEBP_LOSSY -> OsEncodeFormat.WEBP_LOSSY
        ExportFormat.WEBP_LOSSLESS -> OsEncodeFormat.WEBP_LOSSLESS
    }

    private fun Throwable.toUiError(): UiError = when (this) {
        is NativePageDecodeException -> error.toUiError()
        is NativeCodecException -> nativeError.toUiError(UiErrorCode.ENCODE)
        is OsAnimatedPlaybackUnsupportedException -> UiError(UiErrorCode.OS_ANIMATION_UNSUPPORTED)
        is ReadingPlanLimitException -> UiError(UiErrorCode.LIMIT)
        is CredentialInvalidatedException -> UiError(UiErrorCode.KEYSTORE_INVALIDATED)
        is CodecLimitException -> UiError(UiErrorCode.LIMIT)
        is SecurityException -> UiError(UiErrorCode.PERMISSION_REVOKED)
        is SourceException -> UiError(
            when (code) {
                SourceErrorCode.AUTHENTICATION_FAILED -> UiErrorCode.AUTHENTICATION_FAILED
                SourceErrorCode.ACCESS_DENIED -> UiErrorCode.ACCESS_DENIED
                SourceErrorCode.NETWORK -> UiErrorCode.NETWORK
                SourceErrorCode.INTEGRITY -> UiErrorCode.INTEGRITY
                SourceErrorCode.CANCELLED -> UiErrorCode.CANCELLED
                SourceErrorCode.IO -> UiErrorCode.IO
                else -> UiErrorCode.UNKNOWN
            },
        )
        is IllegalArgumentException -> UiError(UiErrorCode.UNKNOWN)
        else -> UiError(UiErrorCode.UNKNOWN)
    }

    private fun NativeRequestError?.toUiError(fallback: UiErrorCode = UiErrorCode.DECODE): UiError = if (this == null) {
        UiError(fallback)
    } else {
        UiError(NativeUiErrorMapper.fromCode(code), safeArguments())
    }

    private fun entryComparator(sortOrder: FilerSortOrder): Comparator<SourceEntry> {
        val kind = compareBy<SourceEntry> { it.kind != EntryKind.DIRECTORY }
        val value = when (sortOrder) {
            FilerSortOrder.NAME_ASCENDING -> compareBy(NaturalFileNameComparator) { it.name }
            FilerSortOrder.NAME_DESCENDING -> compareBy(NaturalFileNameComparator.reversed()) { it.name }
            FilerSortOrder.MODIFIED_DESCENDING -> compareByDescending<SourceEntry> { it.modifiedAtEpochMillis ?: Long.MIN_VALUE }
        }
        return kind.then(value)
    }

    private data class RegisteredSource(
        val sourceId: String,
        val displayName: String,
        val kind: SourceKind,
        val provider: SourceProvider,
        var credentialReentryRequired: Boolean = false,
    )

    private data class NavigationNode(val ref: EntryRef, val label: String)
    private data class PendingOperation(val id: String, val request: FilerOperationRequest)
    private data class PendingExportOperation(
        val id: String,
        val request: ExportRequest,
        val pageId: String,
        val parent: EntryRef,
        val destinationName: String,
        val systemTreeUri: String?,
    )

    private companion object {
        const val MAX_EXPORT_BYTES = 256L * 1024 * 1024
        const val MAX_RESTORED_NAVIGATION_DEPTH = 256
    }
}

internal fun sourceRootRowCapabilities(): SourceCapabilities = SourceCapabilities(canList = true)

internal class ReadingPlanLimitException : Exception("The page set exceeds the mobile reading limit")

internal suspend fun <T> withNonRootSourceOperation(
    target: EntryRef,
    registeredRoot: EntryRef?,
    operation: suspend () -> T,
): T {
    if (target == registeredRoot) {
        throw SourceException(SourceErrorCode.ACCESS_DENIED, "Source roots cannot be modified")
    }
    return operation()
}

internal fun normalizedExportName(value: String, format: ExportFormat): String {
    val leaf = value.trim().substringAfterLast('/').substringAfterLast('\\')
    require(leaf.isNotBlank() && leaf != "." && leaf != ".." && '\u0000' !in leaf) {
        "Invalid export file name"
    }
    val stem = leaf.substringBeforeLast('.', leaf).trim().ifBlank { "wml2viewer" }
    require(stem.length <= 220) { "Export file name is too long" }
    return "$stem.${format.extension}"
}

internal fun exportNameCollides(candidate: String, existingNames: Iterable<String>): Boolean =
    existingNames.any { it == candidate }

internal fun CollisionResolution.toCollisionPolicy(): CollisionPolicy = when (this) {
    CollisionResolution.REPLACE -> CollisionPolicy.REPLACE
    CollisionResolution.KEEP_BOTH -> CollisionPolicy.KEEP_BOTH
    CollisionResolution.SKIP -> CollisionPolicy.SKIP
}

private fun ByteArray.sha256Hex(): String =
    MessageDigest.getInstance("SHA-256").digest(this).toHex()

private fun java.io.InputStream.sha256Hex(): String {
    val digest = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(256 * 1024)
    while (true) {
        val count = read(buffer)
        if (count < 0) break
        digest.update(buffer, 0, count)
    }
    return digest.digest().toHex()
}

private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
