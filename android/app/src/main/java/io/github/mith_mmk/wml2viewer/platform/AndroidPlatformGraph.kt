package io.github.mith_mmk.wml2viewer.platform

import android.content.Context
import android.net.Uri
import io.github.mith_mmk.wml2viewer.data.cache.LruFileCache
import io.github.mith_mmk.wml2viewer.data.source.SourceProvider
import io.github.mith_mmk.wml2viewer.data.source.SourceProviderRegistry
import io.github.mith_mmk.wml2viewer.data.source.SourceTransferEngine
import io.github.mith_mmk.wml2viewer.data.transfer.TransferDatabase
import io.github.mith_mmk.wml2viewer.data.transfer.TransferJobScheduler
import io.github.mith_mmk.wml2viewer.data.transfer.TransferWorkerDependencies
import io.github.mith_mmk.wml2viewer.data.transfer.TransferWorkerRuntime
import io.github.mith_mmk.wml2viewer.platform.codec.AndroidCodecRouter
import io.github.mith_mmk.wml2viewer.platform.codec.RustCodecFallback
import io.github.mith_mmk.wml2viewer.platform.saf.SafSourceProvider
import io.github.mith_mmk.wml2viewer.platform.security.KeystoreCredentialStore
import io.github.mith_mmk.wml2viewer.platform.smb.SmbProfile
import io.github.mith_mmk.wml2viewer.platform.smb.SmbShareEnumerationService
import io.github.mith_mmk.wml2viewer.platform.smb.SmbSourceProvider
import io.github.mith_mmk.wml2viewer.platform.smb.SrvsvcShareEnumerationService
import java.io.File

data class AndroidCacheLimits(
    val maxBytes: Long = 256L * 1024 * 1024,
    val maxEntries: Int = 2_048,
    val maxSingleEntryBytes: Long = maxBytes,
) {
    init {
        require(maxBytes > 0 && maxEntries > 0) { "Invalid Android cache limits" }
        require(maxSingleEntryBytes in 1..maxBytes) { "Invalid single-entry cache limit" }
    }
}

/**
 * Public construction boundary for Activity/ViewModel integration. Persisted SAF
 * permission and profile/config restoration intentionally remain outside this graph.
 */
class AndroidPlatformGraph(
    context: Context,
    private val rustCodecFallback: RustCodecFallback? = null,
    cacheLimits: AndroidCacheLimits = AndroidCacheLimits(),
) : AutoCloseable {
    private val applicationContext = context.applicationContext

    val credentialStore = KeystoreCredentialStore(applicationContext)
    val sourceRegistry = SourceProviderRegistry()
    val transferDatabase = TransferDatabase.build(applicationContext)
    val transferEngine = SourceTransferEngine(sourceRegistry)
    val transferScheduler = TransferJobScheduler(applicationContext, transferDatabase.transferJobs())
    val codecRouter = AndroidCodecRouter(fallback = rustCodecFallback)
    val fileCache = LruFileCache(
        File(applicationContext.cacheDir, CACHE_DIRECTORY),
        cacheLimits.maxBytes,
        cacheLimits.maxEntries,
        cacheLimits.maxSingleEntryBytes,
    )

    fun registerSaf(treeUri: Uri): SafSourceProvider = SafSourceProvider(applicationContext, treeUri).also {
        sourceRegistry.register(it)
    }

    fun registerSmb(
        profile: SmbProfile,
        shareEnumerationService: SmbShareEnumerationService = SrvsvcShareEnumerationService(credentialStore),
    ): SmbSourceProvider = SmbSourceProvider(
        profile,
        credentialStore,
        shareEnumerationService,
    ).also(sourceRegistry::register)

    fun unregister(providerId: String): SourceProvider? = sourceRegistry.unregister(providerId)?.also(SourceProvider::close)

    fun installTransferRuntime() {
        TransferWorkerRuntime.install(
            TransferWorkerDependencies(transferDatabase, transferEngine),
        )
    }

    override fun close() {
        TransferWorkerRuntime.clear()
        sourceRegistry.close()
        transferDatabase.close()
        (rustCodecFallback as? AutoCloseable)?.close()
    }

    private companion object {
        const val CACHE_DIRECTORY = "source-cache-v1"
    }
}
