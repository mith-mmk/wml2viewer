package io.github.mith_mmk.wml2viewer

import android.app.Application
import android.os.StrictMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel

class Wml2ViewerApplication : Application() {
    private val applicationScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private lateinit var componentReady: Deferred<AndroidAppComponent>
    @Volatile
    private var initializedComponent: AndroidAppComponent? = null

    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.DEBUG) {
            StrictMode.setThreadPolicy(
                StrictMode.ThreadPolicy.Builder()
                    .detectAll()
                    .penaltyLog()
                    .build(),
            )
            StrictMode.setVmPolicy(
                StrictMode.VmPolicy.Builder()
                    .detectLeakedClosableObjects()
                    .detectLeakedRegistrationObjects()
                    .penaltyLog()
                    .build(),
            )
        }
        // The one-time legacy tree cleanup may be large, so never wait for it on the main thread.
        componentReady = applicationScope.async {
            LegacyAndroidReset.runOnce(this@Wml2ViewerApplication)
            AndroidAppComponent(this@Wml2ViewerApplication).also { initializedComponent = it }
        }
    }

    suspend fun awaitComponent(): AndroidAppComponent = componentReady.await()

    override fun onTerminate() {
        initializedComponent?.close()
        applicationScope.cancel()
        super.onTerminate()
    }
}
