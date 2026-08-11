package io.github.mith_mmk.wml2viewer.nativebridge

import java.io.Closeable
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** Request-id-aware session wrapper that drops stale native results safely. */
class NativeSession : Closeable {
    private val sessionHandle = NativeBridge.createSession()
    private val closed = AtomicBoolean(false)
    private val currentRequestId = AtomicLong(0L)
    @Volatile
    var lastError: NativeRequestError? = null
        private set

    init {
        check(sessionHandle != 0L) { "Rust session creation failed" }
    }

    fun decode(path: String, mime: String? = null): NativeImage? {
        check(!closed.get()) { "NativeSession is closed" }
        val requestId = beginNextRequest() ?: return null
        val imageHandle = NativeBridge.decode(sessionHandle, requestId, path, mime)
        return finishImageRequest(requestId, imageHandle)
    }

    fun openArchive(path: String, format: NativeArchiveFormat): NativeArchive? {
        check(!closed.get()) { "NativeSession is closed" }
        val requestId = beginNextRequest() ?: return null
        val archiveHandle = NativeBridge.openArchive(
            sessionHandle,
            requestId,
            path,
            format.wireName,
        )
        if (archiveHandle == 0L) {
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        if (!NativeBridge.isRequestCurrent(sessionHandle, requestId)) {
            NativeBridge.releaseArchive(archiveHandle)
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        return NativeArchive.acquire(archiveHandle, this)
    }

    internal fun decodeArchiveEntry(
        archive: NativeArchive,
        index: Int,
        mime: String?,
    ): NativeImage? {
        check(!closed.get()) { "NativeSession is closed" }
        val requestId = beginNextRequest() ?: return null
        val imageHandle = NativeBridge.decodeArchiveEntry(
            sessionHandle,
            requestId,
            archive.nativeHandle,
            index,
            mime,
        )
        return finishImageRequest(requestId, imageHandle)
    }

    internal fun materializeArchiveEntry(
        archive: NativeArchive,
        index: Int,
    ): NativeBytes? {
        check(!closed.get()) { "NativeSession is closed" }
        val requestId = beginNextRequest() ?: return null
        val bytesHandle = NativeBridge.materializeArchiveEntry(
            sessionHandle,
            requestId,
            archive.nativeHandle,
            index,
        )
        return finishBytesRequest(requestId, bytesHandle)
    }

    fun encodeRgba(
        rgba: java.nio.ByteBuffer,
        width: Int,
        height: Int,
        stride: Int,
        format: String,
    ): NativeBytes? {
        check(!closed.get()) { "NativeSession is closed" }
        require(rgba.isDirect) { "RGBA input must be a direct buffer" }
        val requestId = beginNextRequest() ?: return null
        val bytesHandle = NativeBridge.encodeRgba(
            sessionHandle,
            requestId,
            rgba,
            width,
            height,
            stride,
            format,
        )
        return finishBytesRequest(requestId, bytesHandle)
    }

    private fun beginNextRequest(): Long? {
        val requestId = NativeBridge.nextRequestId(sessionHandle)
        if (requestId <= 0L) return null
        currentRequestId.set(requestId)
        lastError = null
        if (NativeBridge.beginRequest(sessionHandle, requestId)) return requestId
        lastError = NativeRequestError.read(sessionHandle, requestId)
        return null
    }

    private fun finishImageRequest(requestId: Long, imageHandle: Long): NativeImage? {
        if (imageHandle == 0L) {
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        if (!NativeBridge.isRequestCurrent(sessionHandle, requestId)) {
            NativeBridge.releaseImage(imageHandle)
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        return NativeImage.acquire(imageHandle)
    }

    private fun finishBytesRequest(requestId: Long, bytesHandle: Long): NativeBytes? {
        if (bytesHandle == 0L) {
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        if (!NativeBridge.isRequestCurrent(sessionHandle, requestId)) {
            NativeBridge.releaseBytes(bytesHandle)
            lastError = NativeRequestError.read(sessionHandle, requestId)
            return null
        }
        return NativeBytes.acquire(bytesHandle)
    }

    fun cancelCurrent(): Boolean {
        if (closed.get()) return false
        val requestId = currentRequestId.get()
        return requestId > 0L && NativeBridge.cancelRequest(sessionHandle, requestId)
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) {
            val requestId = currentRequestId.get()
            if (requestId > 0L) NativeBridge.cancelRequest(sessionHandle, requestId)
            NativeBridge.releaseSession(sessionHandle)
        }
    }
}
