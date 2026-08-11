package io.github.mith_mmk.wml2viewer.nativebridge

import java.io.Closeable
import java.util.concurrent.atomic.AtomicBoolean

enum class NativeArchiveFormat(val wireName: String) {
    ZIP("zip"),
    LHA("lha"),
    LZH("lzh"),
    WMLTXT("wmltxt"),
}

data class NativeArchiveEntry(
    val index: Int,
    val name: String,
    val size: Long?,
)

/** Owns one archive handle and keeps request-aware entry decode behind [NativeSession]. */
class NativeArchive private constructor(
    internal val nativeHandle: Long,
    private val session: NativeSession,
    val entryCount: Int,
) : Closeable {
    private val closed = AtomicBoolean(false)

    fun entry(index: Int): NativeArchiveEntry? {
        check(!closed.get()) { "NativeArchive is closed" }
        require(index in 0 until entryCount) { "index is out of range" }
        val name = NativeBridge.archiveEntryName(nativeHandle, index) ?: return null
        val size = NativeBridge.archiveEntrySize(nativeHandle, index).takeIf { it >= 0L }
        return NativeArchiveEntry(index, name, size)
    }

    fun decodeEntry(index: Int, mime: String? = null): NativeImage? {
        check(!closed.get()) { "NativeArchive is closed" }
        require(index in 0 until entryCount) { "index is out of range" }
        return session.decodeArchiveEntry(this, index, mime)
    }

    fun materializeEntry(index: Int): NativeBytes? {
        check(!closed.get()) { "NativeArchive is closed" }
        require(index in 0 until entryCount) { "index is out of range" }
        return session.materializeArchiveEntry(this, index)
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) {
            NativeBridge.releaseArchive(nativeHandle)
        }
    }

    companion object {
        internal fun acquire(
            handle: Long,
            session: NativeSession,
        ): NativeArchive? {
            if (handle == 0L) return null
            val count = NativeBridge.archiveEntryCount(handle)
            if (count < 0) {
                NativeBridge.releaseArchive(handle)
                return null
            }
            return NativeArchive(handle, session, count)
        }
    }
}
