package io.github.mith_mmk.wml2viewer.data.cache

import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.InputStream
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Size-and-count bounded disk LRU. Keys are hashed, writes are staged, and
 * leased entries are pinned until the lease is closed.
 */
class LruFileCache(
    private val directory: File,
    maxBytes: Long,
    maxEntries: Int,
    maxSingleEntryBytes: Long = maxBytes,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val pins = HashMap<String, Int>()
    private var maxBytes = maxBytes
    private var maxEntries = maxEntries
    private var maxSingleEntryBytes = maxSingleEntryBytes

    init {
        require(maxBytes > 0) { "maxBytes must be positive" }
        require(maxEntries > 0) { "maxEntries must be positive" }
        require(maxSingleEntryBytes in 1..maxBytes) { "Invalid single-entry limit" }
        if (!directory.exists() && !directory.mkdirs()) throw IllegalStateException("Unable to create cache directory")
        require(directory.isDirectory) { "Cache path is not a directory" }
        cleanupTemporaryFiles()
        trim()
    }

    @Synchronized
    fun put(key: String, input: InputStream): CacheLease {
        val id = cacheId(key)
        val destination = dataFile(id)
        val temporary = File(directory, ".$id.${clock()}.tmp")
        var size = 0L
        try {
            FileOutputStream(temporary).use { output ->
                val buffer = ByteArray(BUFFER_SIZE)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    size += count
                    if (size > maxSingleEntryBytes) throw CacheLimitException("Cache entry is too large")
                    output.write(buffer, 0, count)
                }
                output.fd.sync()
            }
            if (!temporary.renameTo(destination)) {
                if (destination.exists() && !destination.delete()) throw IllegalStateException("Unable to replace cache entry")
                if (!temporary.renameTo(destination)) throw IllegalStateException("Unable to publish cache entry")
            }
            touch(destination)
            pin(id)
            try {
                trim(excluding = id)
            } catch (error: Throwable) {
                unpin(id)
                destination.delete()
                throw error
            }
            return lease(id, destination)
        } finally {
            temporary.delete()
        }
    }

    @Synchronized
    fun get(key: String): CacheLease? {
        val id = cacheId(key)
        val file = dataFile(id)
        if (!file.isFile) return null
        touch(file)
        pin(id)
        return lease(id, file)
    }

    @Synchronized
    fun remove(key: String): Boolean {
        val id = cacheId(key)
        if (pins.getOrDefault(id, 0) > 0) return false
        return dataFile(id).delete()
    }

    @Synchronized
    fun clearUnpinned() {
        entries().filter { pins.getOrDefault(it.nameWithoutExtension, 0) == 0 }.forEach(File::delete)
    }

    @Synchronized
    fun snapshot(): CacheSnapshot {
        val entries = entries()
        return CacheSnapshot(entries.sumOf(File::length), entries.size, pins.values.sum())
    }

    /** Applies Auto/manual settings atomically; failed pinned-entry trims restore old limits. */
    @Synchronized
    fun updateLimits(maxBytes: Long, maxEntries: Int, maxSingleEntryBytes: Long = maxBytes) {
        validateLimits(maxBytes, maxEntries, maxSingleEntryBytes)
        val previous = Triple(this.maxBytes, this.maxEntries, this.maxSingleEntryBytes)
        this.maxBytes = maxBytes
        this.maxEntries = maxEntries
        this.maxSingleEntryBytes = maxSingleEntryBytes
        try {
            trim()
        } catch (error: Throwable) {
            this.maxBytes = previous.first
            this.maxEntries = previous.second
            this.maxSingleEntryBytes = previous.third
            throw error
        }
    }

    @Synchronized
    fun trim(excluding: String? = null) {
        val candidates = entries().sortedWith(compareBy<File> { it.lastModified() }.thenBy { it.name })
        var bytes = candidates.sumOf(File::length)
        var count = candidates.size
        for (file in candidates) {
            if (bytes <= maxBytes && count <= maxEntries) break
            val id = file.nameWithoutExtension
            if (id == excluding || pins.getOrDefault(id, 0) > 0) continue
            val length = file.length()
            if (file.delete()) {
                bytes -= length
                count -= 1
            }
        }
        if (bytes > maxBytes || count > maxEntries) {
            throw CacheLimitException("Cache limits cannot be met while entries are pinned")
        }
    }

    private fun lease(id: String, file: File): CacheLease = CacheLease(file) {
        synchronized(this) { unpin(id) }
    }

    private fun pin(id: String) {
        pins[id] = pins.getOrDefault(id, 0) + 1
    }

    private fun unpin(id: String) {
        val count = pins.getOrDefault(id, 0)
        if (count <= 1) pins.remove(id) else pins[id] = count - 1
    }

    private fun entries(): List<File> = directory.listFiles { file ->
        file.isFile && file.name.endsWith(DATA_SUFFIX) && file.nameWithoutExtension.length == HASH_HEX_LENGTH
    }?.toList().orEmpty()

    private fun cleanupTemporaryFiles() {
        directory.listFiles { file -> file.isFile && file.name.endsWith(".tmp") }
            ?.forEach(File::delete)
    }

    private fun dataFile(id: String) = File(directory, "$id$DATA_SUFFIX")

    private fun touch(file: File) {
        if (!file.setLastModified(clock())) throw IllegalStateException("Unable to update cache access time")
    }

    companion object {
        private const val BUFFER_SIZE = 256 * 1024
        private const val DATA_SUFFIX = ".bin"
        private const val HASH_HEX_LENGTH = 64

        fun cacheId(key: String): String {
            require(key.isNotBlank() && key.length <= 16_384) { "Invalid cache key" }
            return MessageDigest.getInstance("SHA-256")
                .digest(key.toByteArray(Charsets.UTF_8))
                .joinToString("") { "%02x".format(it) }
        }

        private fun validateLimits(maxBytes: Long, maxEntries: Int, maxSingleEntryBytes: Long) {
            require(maxBytes > 0) { "maxBytes must be positive" }
            require(maxEntries > 0) { "maxEntries must be positive" }
            require(maxSingleEntryBytes in 1..maxBytes) { "Invalid single-entry limit" }
        }
    }
}

class CacheLease internal constructor(
    val file: File,
    private val release: () -> Unit,
) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    fun openInputStream(): InputStream {
        check(!closed.get()) { "Cache lease is closed" }
        return FileInputStream(file)
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) release()
    }
}

data class CacheSnapshot(val bytes: Long, val entries: Int, val pinnedLeases: Int)

class CacheLimitException(message: String) : Exception(message)
