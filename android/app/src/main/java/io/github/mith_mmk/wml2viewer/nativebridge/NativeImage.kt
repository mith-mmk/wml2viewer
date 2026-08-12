package io.github.mith_mmk.wml2viewer.nativebridge

import java.io.Closeable
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean

/** Owns one Rust image handle. Call [close] as soon as Compose has copied the pixels. */
class NativeImage private constructor(
    private val nativeHandle: Long,
    val width: Int,
    val height: Int,
    val stride: Int,
    val frameCount: Int,
    /** Zero means infinite; -1 means the source did not specify a loop count. */
    val loopCount: Long,
    private val pixels: ByteBuffer,
) : Closeable {
    private val closed = AtomicBoolean(false)

    fun rgba8888(): ByteBuffer {
        check(!closed.get()) { "NativeImage is closed" }
        return pixels.duplicate()
            .order(ByteOrder.nativeOrder())
            .asReadOnlyBuffer()
            .apply { position(0) }
    }

    fun frameDurationMs(frameIndex: Int): Long {
        check(!closed.get()) { "NativeImage is closed" }
        require(frameIndex in 0 until frameCount) { "frameIndex is out of range" }
        return NativeBridge.imageFrameDurationMs(nativeHandle, frameIndex)
    }

    /** Returns an independently owned composited frame. The caller must close it. */
    fun frame(frameIndex: Int): NativeImage? {
        check(!closed.get()) { "NativeImage is closed" }
        require(frameIndex in 0 until frameCount) { "frameIndex is out of range" }
        return acquire(NativeBridge.imageFrame(nativeHandle, frameIndex))
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) {
            NativeBridge.releaseImage(nativeHandle)
        }
    }

    companion object {
        internal fun acquire(handle: Long): NativeImage? {
            if (handle == 0L) return null
            val width = NativeBridge.imageWidth(handle)
            val height = NativeBridge.imageHeight(handle)
            val stride = NativeBridge.imageStride(handle)
            val frameCount = NativeBridge.imageFrameCount(handle)
            val loopCount = NativeBridge.imageLoopCount(handle)
            val pixels = NativeBridge.imageBuffer(handle)
            val valid = width > 0 && height > 0 && stride >= width * 4 && frameCount > 0 &&
                pixels != null && pixels.isDirect && pixels.capacity().toLong() >= stride.toLong() * height
            if (!valid) {
                NativeBridge.releaseImage(handle)
                return null
            }
            return NativeImage(handle, width, height, stride, frameCount, loopCount, pixels!!)
        }
    }
}
