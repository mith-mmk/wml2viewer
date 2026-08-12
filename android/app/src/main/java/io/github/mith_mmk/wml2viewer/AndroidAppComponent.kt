package io.github.mith_mmk.wml2viewer

import io.github.mith_mmk.wml2viewer.data.config.MobileConfigRepository
import io.github.mith_mmk.wml2viewer.data.config.MobileLastLocationStore
import io.github.mith_mmk.wml2viewer.data.config.MobileSourceProfileStore
import io.github.mith_mmk.wml2viewer.data.config.ProtoMobileSettingsStore
import io.github.mith_mmk.wml2viewer.data.controller.AndroidMobileViewerController
import io.github.mith_mmk.wml2viewer.platform.AndroidPlatformGraph
import io.github.mith_mmk.wml2viewer.platform.codec.NativeRustCodecFallback
import io.github.mith_mmk.wml2viewer.ui.state.ViewerViewModelFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

/** Process-scoped dependency graph. It never retains an Activity or Compose object. */
class AndroidAppComponent(
    application: Wml2ViewerApplication,
) : AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val configRepository = MobileConfigRepository(application)
    val settingsStore = ProtoMobileSettingsStore(configRepository, scope)
    private val sourceProfiles = MobileSourceProfileStore(configRepository)
    private val lastLocation = MobileLastLocationStore(configRepository)
    val platform = AndroidPlatformGraph(
        context = application,
        rustCodecFallback = NativeRustCodecFallback(),
    )
    val viewerController = AndroidMobileViewerController(
        context = application,
        graph = platform,
        profileStore = sourceProfiles,
        locationStore = lastLocation,
        settingsStore = settingsStore,
        scope = scope,
    )
    val viewerViewModelFactory = ViewerViewModelFactory(viewerController, settingsStore)

    override fun close() {
        scope.cancel()
        viewerController.close()
        platform.close()
    }
}
