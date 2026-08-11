package io.github.mith_mmk.wml2viewer.data.controller

import android.graphics.Bitmap
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import io.github.mith_mmk.wml2viewer.data.cache.CacheLease
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.nativebridge.NativeArchive
import io.github.mith_mmk.wml2viewer.nativebridge.NativeArchiveFormat
import io.github.mith_mmk.wml2viewer.nativebridge.NativeImage
import io.github.mith_mmk.wml2viewer.nativebridge.NativeRequestError
import io.github.mith_mmk.wml2viewer.nativebridge.NativeSession
import io.github.mith_mmk.wml2viewer.platform.AndroidPlatformGraph
import io.github.mith_mmk.wml2viewer.platform.codec.DecodeOptions
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.state.MobileSettingsStore
import java.util.IdentityHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield

internal sealed interface ViewerPageSource {
    val id: String
    val name: String
    val mimeType: String?
    val sourceBoundary: String

    data class Direct(
        val entry: SourceEntry,
        override val id: String = EntryUiTokenCodec.encode(entry.ref),
        override val sourceBoundary: String = entry.ref.providerId,
    ) : ViewerPageSource {
        override val name: String = entry.name
        override val mimeType: String? = MobileFileTypes.mimeType(entry.name, entry.mimeType)
    }

    data class ArchiveEntry(
        val archive: NativeArchive,
        val archiveIdentity: String,
        val entryIndex: Int,
        override val name: String,
        override val mimeType: String? = MobileFileTypes.mimeType(name),
        override val id: String = "a1:$archiveIdentity:$entryIndex",
    ) : ViewerPageSource {
        override val sourceBoundary: String = archiveIdentity
    }

    data class ListedEntry(
        val baseDirectory: EntryRef,
        val relativePath: String,
        val listedIdentity: String,
        val entryIndex: Int,
        override val id: String = "l1:$listedIdentity:$entryIndex",
    ) : ViewerPageSource {
        override val name: String = relativePath.substringAfterLast('/')
        override val mimeType: String? = MobileFileTypes.mimeType(name)
        override val sourceBoundary: String = listedIdentity
    }
}

internal data class LoadedViewerPage(
    val page: ViewerPageSource,
    val frame: ImageBitmap,
    val portrait: Boolean,
    val animationFrames: List<LoadedAnimationFrame> = emptyList(),
    val animationSource: LoadedAnimationSource? = null,
    val loopCount: Long = -1L,
    /** True when ImageDecoder reported animation but returned only its poster bitmap. */
    val osAnimatedPoster: Boolean = false,
) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    val animationFrameCount: Int
        get() = animationSource?.frameCount ?: animationFrames.size

    fun animationFrame(index: Int): LoadedAnimationFrame =
        animationFrames.getOrNull(index) ?: animationSource?.frame(index)
        ?: throw IndexOutOfBoundsException("Animation frame $index is unavailable")

    override fun close() {
        // Compose snapshots may still reference a displayed Bitmap after cache
        // eviction. Drop cache ownership and close native RGBA deterministically,
        // but let Android reclaim shared Bitmap storage when UI references vanish.
        if (closed.compareAndSet(false, true)) animationSource?.close()
    }
}

internal fun LoadedViewerPage.estimatedRetainedBytes(): Long {
    val bitmaps = IdentityHashMap<ImageBitmap, Unit>()
    var retained = 0L
    fun retain(bitmap: ImageBitmap) {
        if (bitmaps.put(bitmap, Unit) == null) {
            retained = ViewerDecodeMemoryPolicy.saturatingAdd(
                retained,
                ViewerDecodeMemoryPolicy.rgbaBytes(bitmap.width, bitmap.height),
            )
        }
    }
    retain(frame)
    animationFrames.forEach { retain(it.frame) }
    return ViewerDecodeMemoryPolicy.saturatingAdd(
        retained,
        animationSource?.retainedRgbaBytes ?: 0L,
    )
}

internal interface LoadedAnimationSource : AutoCloseable {
    val frameCount: Int
    val retainedRgbaBytes: Long
    fun frame(index: Int): LoadedAnimationFrame
}

internal data class LoadedAnimationFrame(
    val frame: ImageBitmap,
    val durationMillis: Long,
)

internal object AnimationPlaybackPolicy {
    const val MAX_BUFFERED_FRAMES = 64
    // Native RGBA and copied Bitmap pixels coexist while buffering. Keeping
    // each side near 64 MiB bounds that conversion below the 128 MiB cache budget.
    const val MAX_BUFFERED_PIXELS = 16_777_216L

    enum class Storage { NONE, BUFFER, STREAM }

    fun storage(width: Int, height: Int, frameCount: Int): Storage {
        if (width <= 0 || height <= 0 || frameCount < 2) return Storage.NONE
        return if (canBuffer(width, height, frameCount)) Storage.BUFFER else Storage.STREAM
    }

    fun canBuffer(width: Int, height: Int, frameCount: Int): Boolean {
        if (width <= 0 || height <= 0 || frameCount !in 2..MAX_BUFFERED_FRAMES) return false
        val pixels = width.toLong() * height.toLong()
        return pixels > 0L && pixels <= MAX_BUFFERED_PIXELS / frameCount
    }

    fun durationMillis(value: Long): Long = if (value <= 0L) 100L else value.coerceAtMost(60_000L)

    /** Zero is infinite, a missing count plays once, and positive values are total passes. */
    fun playbackPasses(loopCount: Long): Long = when {
        loopCount == 0L -> Long.MAX_VALUE
        loopCount < 0L -> 1L
        else -> loopCount
    }
}

internal object ViewerDecodeMemoryPolicy {
    const val MAX_PIXELS = 4_096L * 4_096L
    const val MAX_FRAME_CACHE_BYTES = 128L * 1024L * 1024L
    const val MAX_COPY_TILE_WIDTH = 4_096
    const val MAX_COPY_TILE_PIXELS = 256 * 1_024
    val OS_DECODE_OPTIONS = DecodeOptions(maxPixels = MAX_PIXELS)

    fun requirePixelCount(width: Int, height: Int): Long {
        require(width > 0 && height > 0) { "Image dimensions must be positive" }
        val pixels = width.toLong() * height.toLong()
        require(pixels <= MAX_PIXELS) { "Image exceeds pixel limit" }
        return pixels
    }

    fun rgbaBytes(width: Int, height: Int): Long = requirePixelCount(width, height) * 4L

    fun saturatingAdd(left: Long, right: Long): Long = when {
        left < 0L || right < 0L -> Long.MAX_VALUE
        left > Long.MAX_VALUE - right -> Long.MAX_VALUE
        else -> left + right
    }

    fun saturatingMultiply(left: Long, right: Long): Long = when {
        left < 0L || right < 0L -> Long.MAX_VALUE
        left == 0L || right == 0L -> 0L
        left > Long.MAX_VALUE / right -> Long.MAX_VALUE
        else -> left * right
    }
}

internal class NativePageDecodeException(
    val error: NativeRequestError?,
) : Exception("Native image decode failed")

/** Access-ordered cache with deterministic close and independent count/byte limits. */
internal class CloseableWeightedLruCache<K, V : AutoCloseable>(
    private val maxEntries: Int,
    private val maxWeight: Long,
    private val weigh: (V) -> Long,
) : AutoCloseable {
    private data class WeightedValue<V>(val value: V, val weight: Long)

    private val entries = LinkedHashMap<K, WeightedValue<V>>(maxEntries + 1, 0.75f, true)
    private var totalWeight = 0L

    init {
        require(maxEntries > 0) { "maxEntries must be positive" }
        require(maxWeight > 0L) { "maxWeight must be positive" }
    }

    @get:Synchronized
    val retainedWeight: Long
        get() = totalWeight

    @get:Synchronized
    val size: Int
        get() = entries.size

    @Synchronized
    operator fun get(key: K): V? = entries[key]?.value

    /** Returns false without taking ownership when one value exceeds the byte budget. */
    @Synchronized
    fun put(key: K, value: V): Boolean {
        val weight = weigh(value)
        require(weight >= 0L) { "Cache weight must not be negative" }
        if (weight > maxWeight) return false

        val previous = entries.remove(key)
        if (previous != null) {
            totalWeight -= previous.weight
            if (previous.value !== value) previous.value.close()
        }
        while (entries.isNotEmpty() &&
            (entries.size >= maxEntries || totalWeight > maxWeight - weight)
        ) {
            evictEldest()
        }
        entries[key] = WeightedValue(value, weight)
        totalWeight += weight
        return true
    }

    @Synchronized
    fun removeIf(predicate: (K, V) -> Boolean) {
        val iterator = entries.entries.iterator()
        while (iterator.hasNext()) {
            val entry = iterator.next()
            if (predicate(entry.key, entry.value.value)) {
                iterator.remove()
                totalWeight -= entry.value.weight
                entry.value.value.close()
            }
        }
    }

    @Synchronized
    fun clear() {
        val values = entries.values.map { it.value }
        entries.clear()
        totalWeight = 0L
        values.forEach(AutoCloseable::close)
    }

    override fun close() = clear()

    private fun evictEldest() {
        val iterator = entries.entries.iterator()
        val eldest = iterator.next()
        iterator.remove()
        totalWeight -= eldest.value.weight
        eldest.value.value.close()
    }
}

/**
 * Owns independent active/prefetch lanes and their native sessions.
 *
 * A cancelled native call can remain blocked in JNI until its decoder observes
 * the cancellation probe. Keeping both the dispatcher permit and session out
 * of the active lane prevents that old prefetch from delaying navigation.
 */
internal class ViewerDecodeExecution<S : AutoCloseable>(
    activeDispatcher: CoroutineDispatcher,
    prefetchDispatcher: CoroutineDispatcher,
    private val sessionFactory: () -> S,
    private val cancelSession: (S) -> Unit,
) : AutoCloseable {
    private class Lane<S>(
        val dispatcher: CoroutineDispatcher,
    ) {
        val mutex = Mutex()
        var session: S? = null
    }

    private val lifecycleLock = Any()
    private val closed = AtomicBoolean(false)
    private val cancellationGeneration = AtomicLong(0L)
    private val activeLane = Lane<S>(activeDispatcher)
    private val prefetchLane = Lane<S>(prefetchDispatcher)

    suspend fun <T> active(block: suspend (S) -> T): T = run(activeLane, block = block)

    suspend fun <T> prefetch(block: suspend (S) -> T): T =
        run(prefetchLane, yieldBeforeRun = true, block = block)

    fun cancelAll() {
        cancellationGeneration.incrementAndGet()
        val (active, prefetch) = synchronized(lifecycleLock) {
            activeLane.session to prefetchLane.session.also { prefetchLane.session = null }
        }
        active?.let(cancelSession)
        // Prefetch never publishes a streaming native handle. Closing and
        // replacing its session closes the race where cancellation arrives
        // immediately before the old block begins its first native request.
        prefetch?.let { session ->
            cancelSession(session)
            session.close()
        }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        val sessions = synchronized(lifecycleLock) {
            listOfNotNull(activeLane.session, prefetchLane.session).also {
                activeLane.session = null
                prefetchLane.session = null
            }
        }
        sessions.forEach { session ->
            cancelSession(session)
            session.close()
        }
    }

    private suspend fun <T> run(
        lane: Lane<S>,
        yieldBeforeRun: Boolean = false,
        block: suspend (S) -> T,
    ): T {
        val acceptedGeneration = cancellationGeneration.get()
        return withContext(lane.dispatcher) {
            // Cooperatively give already-queued active work the first opportunity
            // when both limited views share Dispatchers.IO underneath.
            if (yieldBeforeRun) yield()
            lane.mutex.withLock {
                val session = synchronized(lifecycleLock) {
                    if (closed.get()) throw CancellationException("Viewer decode execution is closed")
                    if (acceptedGeneration != cancellationGeneration.get()) {
                        throw CancellationException("Viewer decode execution was cancelled")
                    }
                    lane.session ?: sessionFactory().also { lane.session = it }
                }
                block(session)
            }
        }
    }
}

/** Materializes provider entries on demand and owns every native image/archive handle it opens. */
internal class ViewerPageLoader(
    private val graph: AndroidPlatformGraph,
    private val settingsStore: MobileSettingsStore,
    private val viewerDispatcher: CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1),
    private val prefetchDispatcher: CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1),
) : AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val retentionLock = Any()
    private val archiveLock = Any()
    private val decodeExecution = ViewerDecodeExecution(
        activeDispatcher = viewerDispatcher,
        prefetchDispatcher = prefetchDispatcher,
        sessionFactory = ::NativeSession,
        cancelSession = { it.cancelCurrent() },
    )
    private val frameCache = CloseableWeightedLruCache<String, LoadedViewerPage>(
        maxEntries = MAX_MEMORY_FRAMES,
        maxWeight = ViewerDecodeMemoryPolicy.MAX_FRAME_CACHE_BYTES,
        weigh = LoadedViewerPage::estimatedRetainedBytes,
    )
    private var uncachedPage: LoadedViewerPage? = null
    private var archive: NativeArchive? = null

    suspend fun openArchive(entry: SourceEntry, format: String): List<ViewerPageSource.ArchiveEntry> =
        decodeExecution.active { session ->
            closeArchive()
            materialize(entry).use { lease ->
                val nativeFormat = when (format) {
                    "zip" -> NativeArchiveFormat.ZIP
                    "lha" -> NativeArchiveFormat.LHA
                    "lzh" -> NativeArchiveFormat.LZH
                    "wmltxt" -> NativeArchiveFormat.WMLTXT
                    else -> throw IllegalArgumentException("Unsupported archive format")
                }
                val opened = session.openArchive(lease.file.absolutePath, nativeFormat)
                    ?: throw NativePageDecodeException(session.lastError)
                synchronized(archiveLock) {
                    if (closed.get()) {
                        opened.close()
                        throw CancellationException("Viewer page loader is closed")
                    }
                    archive = opened
                }
                val identity = EntryUiTokenCodec.encode(entry.ref)
                buildList {
                    repeat(opened.entryCount) { index ->
                        val item = opened.entry(index) ?: return@repeat
                        if (MobileFileTypes.isImage(item.name, null)) {
                            add(
                                ViewerPageSource.ArchiveEntry(
                                    archive = opened,
                                    archiveIdentity = identity,
                                    entryIndex = index,
                                    name = item.name,
                                ),
                            )
                        }
                    }
                }.sortedWith(compareBy(NaturalFileNameComparator) { it.name })
            }
        }

    suspend fun load(page: ViewerPageSource): LoadedViewerPage = decodeExecution.active { session ->
        retainedPageForActiveLoad(page)?.let { return@active it }
        val loaded = when (page) {
            is ViewerPageSource.Direct -> loadDirect(page, session)
            is ViewerPageSource.ArchiveEntry -> loadArchive(page, session)
            is ViewerPageSource.ListedEntry -> loadListed(page, session)
        }
        if (!retainLoadedPage(loaded)) {
            loaded.close()
            throw CancellationException("Viewer page loader is closed")
        }
        loaded
    }

    /**
     * Preload orientation/static pixels without retaining a large streaming animation.
     * Archive handles are bound to the session that opened them, so an uncached archive
     * page is intentionally skipped instead of borrowing the active session.
     */
    suspend fun prefetch(page: ViewerPageSource): Boolean {
        retainedPortrait(page.id)?.let { return it }
        if (page is ViewerPageSource.ArchiveEntry) return true
        return decodeExecution.prefetch { session ->
            retainedPortrait(page.id)?.let { return@prefetch it }
            val loaded = when (page) {
                is ViewerPageSource.Direct -> loadDirect(page, session)
                is ViewerPageSource.ListedEntry -> loadListed(page, session)
                is ViewerPageSource.ArchiveEntry -> error("Archive prefetch must use the active session")
            }
            retainPrefetchedPage(loaded)
        }
    }

    fun cancelCurrent() {
        decodeExecution.cancelAll()
    }

    fun clearFrames() {
        synchronized(retentionLock) {
            frameCache.clear()
            uncachedPage?.close()
            uncachedPage = null
        }
    }

    suspend fun leaveArchive() = decodeExecution.active {
        closeArchive()
    }

    private suspend fun loadDirect(
        page: ViewerPageSource.Direct,
        session: NativeSession,
    ): LoadedViewerPage {
        val policy = effectivePolicy(page.mimeType)
        return materialize(page.entry).use { lease ->
            when (policy) {
                CodecPolicy.OS_ONLY -> loadWithOs(page, lease).requireFullOsPlayback()
                CodecPolicy.OS_FIRST -> withOsAnimationCompatibility(
                    os = { loadWithOs(page, lease) },
                    internal = { loadWithNative(page, lease, session) },
                )
                CodecPolicy.INTERNAL_ONLY -> loadWithNative(page, lease, session)
                CodecPolicy.INTERNAL_FIRST, CodecPolicy.DEFAULT -> preferInternalWithStaticOsFallback(
                    internal = { loadWithNative(page, lease, session) },
                    os = { loadWithOs(page, lease) },
                )
            }
        }
    }

    private suspend fun loadListed(
        page: ViewerPageSource.ListedEntry,
        session: NativeSession,
    ): LoadedViewerPage {
        val components = page.relativePath.split('/')
        require(components.isNotEmpty() && components.all {
            it.isNotBlank() && it != "." && it != ".." && '\u0000' !in it
        }) { "Invalid listed-file entry" }
        val provider = graph.sourceRegistry.require(page.baseDirectory)
        var parent = page.baseDirectory
        var resolved: SourceEntry? = null
        components.forEachIndexed { index, component ->
            resolved = provider.list(parent).firstOrNull { it.name == component }
                ?: throw SourceException(SourceErrorCode.NOT_FOUND, "Listed-file entry is unavailable")
            if (index < components.lastIndex) {
                if (resolved?.kind != EntryKind.DIRECTORY) {
                    throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Listed-file parent is not a directory")
                }
                parent = resolved!!.ref
            }
        }
        val entry = resolved?.takeIf { it.kind == EntryKind.FILE }
            ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Listed-file entry is not a file")
        val direct = ViewerPageSource.Direct(entry, page.id, page.sourceBoundary)
        return loadDirect(direct, session).copy(page = page)
    }

    private suspend fun loadArchive(
        page: ViewerPageSource.ArchiveEntry,
        session: NativeSession,
    ): LoadedViewerPage {
        val policy = effectivePolicy(page.mimeType)
        return when (policy) {
            CodecPolicy.OS_ONLY -> loadArchiveWithOs(page, session).requireFullOsPlayback()
            CodecPolicy.OS_FIRST -> withOsAnimationCompatibility(
                os = { loadArchiveWithOs(page, session) },
                internal = { loadArchiveWithNative(page, session) },
            )
            CodecPolicy.INTERNAL_ONLY -> loadArchiveWithNative(page, session)
            CodecPolicy.INTERNAL_FIRST, CodecPolicy.DEFAULT -> preferInternalWithStaticOsFallback(
                internal = { loadArchiveWithNative(page, session) },
                os = { loadArchiveWithOs(page, session) },
            )
        }
    }

    private suspend fun withOsAnimationCompatibility(
        os: suspend () -> LoadedViewerPage,
        internal: suspend () -> LoadedViewerPage,
    ): LoadedViewerPage = preferInternalForAnimatedOsPoster(os, internal)

    private fun LoadedViewerPage.requireFullOsPlayback(): LoadedViewerPage {
        if (osAnimatedPoster) throw OsAnimatedPlaybackUnsupportedException()
        return this
    }

    private fun loadArchiveWithNative(
        page: ViewerPageSource.ArchiveEntry,
        session: NativeSession,
    ): LoadedViewerPage {
        val image = page.archive.decodeEntry(page.entryIndex, page.mimeType)
            ?: throw NativePageDecodeException(session.lastError)
        return image.consumeToLoadedPage(page, session)
    }

    private suspend fun loadArchiveWithOs(
        page: ViewerPageSource.ArchiveEntry,
        session: NativeSession,
    ): LoadedViewerPage {
        val encoded = page.archive.materializeEntry(page.entryIndex)
            ?: throw NativePageDecodeException(session.lastError)
        return encoded.use { bytes ->
            val decoded = graph.codecRouter.decode(
                bytes.readOnlyBuffer(),
                page.mimeType,
                ViewerDecodeMemoryPolicy.OS_DECODE_OPTIONS,
            )
            decoded.use { image ->
                val bitmap = image.bitmap.copyArgb8888ForViewer()
                LoadedViewerPage(
                    page,
                    bitmap.asImageBitmap(),
                    bitmap.height >= bitmap.width,
                    osAnimatedPoster = image.animatedSource,
                )
            }
        }
    }

    private fun loadWithNative(
        page: ViewerPageSource.Direct,
        lease: CacheLease,
        session: NativeSession,
    ): LoadedViewerPage {
        val image = session.decode(lease.file.absolutePath, page.mimeType)
            ?: throw NativePageDecodeException(session.lastError)
        return image.consumeToLoadedPage(page, session)
    }

    private suspend fun loadWithOs(
        page: ViewerPageSource.Direct,
        lease: CacheLease,
    ): LoadedViewerPage {
        val decoded = graph.codecRouter.decode(
            lease.file,
            page.mimeType,
            ViewerDecodeMemoryPolicy.OS_DECODE_OPTIONS,
        )
        return decoded.use { image ->
            val bitmap = image.bitmap.copyArgb8888ForViewer()
            LoadedViewerPage(
                page,
                bitmap.asImageBitmap(),
                bitmap.height >= bitmap.width,
                osAnimatedPoster = image.animatedSource,
            )
        }
    }

    private suspend fun materialize(entry: SourceEntry): CacheLease = withContext(Dispatchers.IO) {
        val key = buildString {
            append(entry.ref.providerId)
            append('\u0000')
            append(entry.ref.opaqueId)
            append('\u0000')
            append(entry.size ?: -1L)
            append('\u0000')
            append(entry.modifiedAtEpochMillis ?: -1L)
        }
        graph.fileCache.get(key) ?: graph.sourceRegistry.require(entry.ref).openRead(entry.ref).use { read ->
            read.stream.use { input -> graph.fileCache.put(key, input) }
        }
    }

    private fun effectivePolicy(mimeType: String?): CodecPolicy {
        val settings = settingsStore.settings.value.codecs
        val format = when (io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.fromMimeType(mimeType)) {
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.JPEG -> CodecFormat.JPEG
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.PNG -> CodecFormat.PNG
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.GIF -> CodecFormat.GIF
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.WEBP -> CodecFormat.WEBP
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.BMP -> CodecFormat.BMP
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.ICO -> CodecFormat.ICO
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.HEIF -> CodecFormat.HEIF
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.AVIF -> CodecFormat.AVIF
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.DNG -> CodecFormat.DNG
            io.github.mith_mmk.wml2viewer.platform.codec.CodecFormat.UNKNOWN -> null
        }
        val override = format?.let(settings::policyFor) ?: CodecPolicy.DEFAULT
        return if (override == CodecPolicy.DEFAULT) settings.defaultPolicy else override
    }

    private fun NativeImage.consumeToLoadedPage(
        page: ViewerPageSource,
        session: NativeSession,
    ): LoadedViewerPage {
        var ownershipTransferred = false
        try {
            val poster = copyBitmap()
            return when (AnimationPlaybackPolicy.storage(width, height, frameCount)) {
                AnimationPlaybackPolicy.Storage.NONE -> LoadedViewerPage(
                    page = page,
                    frame = poster,
                    portrait = height >= width,
                )
                AnimationPlaybackPolicy.Storage.BUFFER -> {
                    val frames = ArrayList<LoadedAnimationFrame>(frameCount)
                    for (index in 0 until frameCount) {
                        val bitmap = if (index == 0) {
                            poster
                        } else {
                            frame(index)?.use { it.copyBitmap() }
                                ?: throw NativePageDecodeException(session.lastError)
                        }
                        frames += LoadedAnimationFrame(
                            frame = bitmap,
                            durationMillis = AnimationPlaybackPolicy.durationMillis(frameDurationMs(index)),
                        )
                    }
                    LoadedViewerPage(
                        page = page,
                        frame = poster,
                        portrait = height >= width,
                        animationFrames = frames,
                        loopCount = loopCount,
                    )
                }
                AnimationPlaybackPolicy.Storage.STREAM -> {
                    val source = NativeAnimationSource(this, poster, session)
                    ownershipTransferred = true
                    LoadedViewerPage(
                        page = page,
                        frame = poster,
                        portrait = height >= width,
                        animationSource = source,
                        loopCount = loopCount,
                    )
                }
            }
        } finally {
            if (!ownershipTransferred) close()
        }
    }

    private inner class NativeAnimationSource(
        private val image: NativeImage,
        private val poster: ImageBitmap,
        private val session: NativeSession,
    ) : LoadedAnimationSource {
        @Volatile
        private var closed = false

        override val frameCount: Int
            get() = synchronized(this) {
                check(!closed) { "Animation source is closed" }
                image.frameCount
            }

        override val retainedRgbaBytes: Long
            get() = synchronized(this) {
                check(!closed) { "Animation source is closed" }
                val canvasBytes = ViewerDecodeMemoryPolicy.saturatingMultiply(
                    image.stride.toLong(),
                    image.height.toLong(),
                )
                ViewerDecodeMemoryPolicy.saturatingMultiply(
                    canvasBytes,
                    image.frameCount.toLong() + 1L,
                )
            }

        override fun frame(index: Int): LoadedAnimationFrame = synchronized(this) {
            check(!closed) { "Animation source is closed" }
            require(index in 0 until image.frameCount) { "Animation frame is out of range" }
            val bitmap = if (index == 0) {
                poster
            } else {
                image.frame(index)?.use { it.copyBitmap() }
                    ?: throw NativePageDecodeException(session.lastError)
            }
            LoadedAnimationFrame(
                frame = bitmap,
                durationMillis = AnimationPlaybackPolicy.durationMillis(image.frameDurationMs(index)),
            )
        }

        override fun close() = synchronized(this) {
            if (closed) return@synchronized
            closed = true
            image.close()
        }
    }

    private fun NativeImage.copyBitmap(): ImageBitmap {
        ViewerDecodeMemoryPolicy.requirePixelCount(width, height)
        val buffer = rgba8888()
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try {
            // A full-size IntArray adds 64 MiB at the 4K-square cap. Convert in
            // at-most-1-MiB tiles and batch rows to avoid both that peak and a
            // setPixels call per scanline.
            val tileWidth = minOf(width, ViewerDecodeMemoryPolicy.MAX_COPY_TILE_WIDTH)
            val tileRows = minOf(
                height,
                maxOf(1, ViewerDecodeMemoryPolicy.MAX_COPY_TILE_PIXELS / tileWidth),
            )
            val tilePixels = IntArray(tileWidth * tileRows)
            var startY = 0
            while (startY < height) {
                val rows = minOf(tileRows, height - startY)
                var startX = 0
                while (startX < width) {
                    val columns = minOf(tileWidth, width - startX)
                    for (tileY in 0 until rows) {
                        val row = (startY + tileY) * stride + startX * 4
                        val destinationRow = tileY * columns
                        for (tileX in 0 until columns) {
                            val offset = row + tileX * 4
                            val red = buffer.get(offset).toInt() and 0xFF
                            val green = buffer.get(offset + 1).toInt() and 0xFF
                            val blue = buffer.get(offset + 2).toInt() and 0xFF
                            val alpha = buffer.get(offset + 3).toInt() and 0xFF
                            tilePixels[destinationRow + tileX] =
                                (alpha shl 24) or (red shl 16) or (green shl 8) or blue
                        }
                    }
                    bitmap.setPixels(
                        tilePixels,
                        0,
                        columns,
                        startX,
                        startY,
                        columns,
                        rows,
                    )
                    startX += columns
                }
                startY += rows
            }
            return bitmap.asImageBitmap()
        } catch (error: Throwable) {
            bitmap.recycle()
            throw error
        }
    }

    private fun Bitmap.copyArgb8888ForViewer(): Bitmap {
        ViewerDecodeMemoryPolicy.requirePixelCount(width, height)
        return checkNotNull(copy(Bitmap.Config.ARGB_8888, false)) {
            "Unable to copy OS decoded bitmap"
        }
    }

    private fun retainedPageForActiveLoad(page: ViewerPageSource): LoadedViewerPage? =
        synchronized(retentionLock) {
            if (closed.get()) throw CancellationException("Viewer page loader is closed")
            // A streaming animation can retain up to the native aggregate limit.
            // Keep only the one belonging to the page being actively requested.
            frameCache.removeIf { id, value -> id != page.id && value.animationSource != null }
            uncachedPage?.let { value ->
                if (value.page.id == page.id) return@synchronized value
                if (value.animationSource != null) {
                    value.close()
                    uncachedPage = null
                }
            }
            frameCache[page.id]
        }

    private fun retainedPortrait(pageId: String): Boolean? = synchronized(retentionLock) {
        if (closed.get()) throw CancellationException("Viewer page loader is closed")
        uncachedPage?.takeIf { it.page.id == pageId }?.portrait ?: frameCache[pageId]?.portrait
    }

    /** Returns the retained/canonical orientation and closes an unowned decoded page. */
    private fun retainPrefetchedPage(loaded: LoadedViewerPage): Boolean =
        synchronized(retentionLock) {
            if (closed.get()) {
                loaded.close()
                throw CancellationException("Viewer page loader is closed")
            }
            val existing = uncachedPage?.takeIf { it.page.id == loaded.page.id }
                ?: frameCache[loaded.page.id]
            if (existing != null) {
                loaded.close()
                return@synchronized existing.portrait
            }
            val portrait = loaded.portrait
            if (uncachedPage != null ||
                loaded.animationSource != null ||
                !frameCache.put(loaded.page.id, loaded)
            ) {
                loaded.close()
            }
            portrait
        }

    /** False means close won the race and the caller still owns [loaded]. */
    private fun retainLoadedPage(loaded: LoadedViewerPage): Boolean = synchronized(retentionLock) {
        if (closed.get()) return@synchronized false
        val previous = uncachedPage
        uncachedPage = if (frameCache.put(loaded.page.id, loaded)) {
            null
        } else {
            // A live animation may legitimately exceed the cache budget. Keep
            // only that displayed page and prevent prefetch/cache residency from
            // adding another 128 MiB beside it.
            frameCache.clear()
            loaded
        }
        if (previous !== loaded) previous?.close()
        true
    }

    private fun closeArchive() {
        val opened = synchronized(archiveLock) {
            archive.also { archive = null }
        }
        opened?.close()
        clearFrames()
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        decodeExecution.close()
        closeArchive()
    }

    private companion object {
        const val MAX_MEMORY_FRAMES = 6
    }
}

/** Keeps native decode, archive I/O, and bitmap copies away from the caller's Main dispatcher. */
internal suspend fun <T> runOnViewerDispatcher(
    dispatcher: CoroutineDispatcher,
    block: suspend () -> T,
): T = withContext(dispatcher) { block() }

internal class OsAnimatedPlaybackUnsupportedException(cause: Throwable? = null) :
    Exception("The OS codec cannot expose animation frames for this viewer", cause)

internal suspend fun preferInternalWithStaticOsFallback(
    internal: suspend () -> LoadedViewerPage,
    os: suspend () -> LoadedViewerPage,
): LoadedViewerPage = try {
    internal()
} catch (error: Throwable) {
    if (error is CancellationException) throw error
    val osPage = os()
    if (osPage.osAnimatedPoster) throw OsAnimatedPlaybackUnsupportedException(error)
    osPage
}

/**
 * OS-first still preserves animation when the internal decoder supports it.
 * A flattened OS poster is never returned as successful animation playback.
 */
internal suspend fun preferInternalForAnimatedOsPoster(
    os: suspend () -> LoadedViewerPage,
    internal: suspend () -> LoadedViewerPage,
): LoadedViewerPage {
    val osPage = try {
        os()
    } catch (error: Throwable) {
        if (error is CancellationException) throw error
        return internal()
    }
    if (!osPage.osAnimatedPoster) return osPage
    return try {
        internal()
    } catch (error: Throwable) {
        if (error is CancellationException) throw error
        throw OsAnimatedPlaybackUnsupportedException(error)
    }
}
