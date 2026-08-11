package io.github.mith_mmk.wml2viewer.nativebridge

import java.io.Closeable
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

/** Owns one Rust byte-buffer handle. The direct view is valid only until [close]. */
class NativeBytes private constructor(
    private val nativeHandle: Long,
    val length: Long,
    private val buffer: ByteBuffer,
) : Closeable {
    private val closed = AtomicBoolean(false)

    fun readOnlyBuffer(): ByteBuffer {
        check(!closed.get()) { "NativeBytes is closed" }
        return buffer.duplicate().asReadOnlyBuffer().apply {
            position(0)
            limit(length.toInt())
        }
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) {
            NativeBridge.releaseBytes(nativeHandle)
        }
    }

    companion object {
        internal fun acquire(handle: Long): NativeBytes? {
            if (handle == 0L) return null
            val length = NativeBridge.bytesLength(handle)
            val buffer = NativeBridge.bytesBuffer(handle)
            val valid = length in 0..Int.MAX_VALUE.toLong() && buffer != null &&
                buffer.isDirect && buffer.capacity().toLong() >= length
            if (!valid) {
                NativeBridge.releaseBytes(handle)
                return null
            }
            return NativeBytes(handle, length, buffer!!)
        }
    }
}
