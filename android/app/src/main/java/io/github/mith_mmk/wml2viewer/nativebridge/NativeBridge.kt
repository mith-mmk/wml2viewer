package io.github.mith_mmk.wml2viewer.nativebridge

import java.nio.ByteBuffer

/** Stable Kotlin declarations for the Rust JNI boundary. */
object NativeBridge {
    init {
        System.loadLibrary("wml2viewer_android")
    }

    external fun createSession(): Long
    external fun releaseSession(sessionHandle: Long): Boolean
    external fun nextRequestId(sessionHandle: Long): Long
    external fun beginRequest(sessionHandle: Long, requestId: Long): Boolean
    external fun cancelRequest(sessionHandle: Long, requestId: Long): Boolean

    /**
     * Returns reading-plan wire v1, or null for an invalid request. Layout is
     * 0=auto/1=single/2=spread and direction is 0=LTR/1=RTL. App code uses
     * [NativeReadingPlanner] so these ABI integers remain inside this package.
     */
    external fun planReading(
        sourceIds: LongArray,
        portrait: BooleanArray,
        covers: BooleanArray,
        currentIndex: Int,
        landscape: Boolean,
        layout: Int,
        direction: Int,
        coverAlone: Boolean,
        maxPrefetchSpreads: Int,
    ): IntArray?

    /** Returns an owned image handle, or 0 when decoding failed or the request became stale. */
    external fun decode(
        sessionHandle: Long,
        requestId: Long,
        path: String,
        mime: String?,
    ): Long

    external fun imageWidth(imageHandle: Long): Int
    external fun imageHeight(imageHandle: Long): Int
    external fun imageStride(imageHandle: Long): Int
    external fun imageBuffer(imageHandle: Long): ByteBuffer?
    external fun imageFrameCount(imageHandle: Long): Int
    external fun imageLoopCount(imageHandle: Long): Long
    external fun imageFrameDurationMs(imageHandle: Long, frameIndex: Int): Long
    external fun imageFrame(imageHandle: Long, frameIndex: Int): Long
    external fun releaseImage(imageHandle: Long): Boolean
    external fun isRequestCurrent(sessionHandle: Long, requestId: Long): Boolean
    external fun requestErrorCode(sessionHandle: Long, requestId: Long): Int
    external fun requestErrorKey(sessionHandle: Long, requestId: Long): String?
    external fun requestErrorArgsJson(sessionHandle: Long, requestId: Long): String?

    external fun openArchive(
        sessionHandle: Long,
        requestId: Long,
        path: String,
        format: String,
    ): Long
    external fun releaseArchive(archiveHandle: Long): Boolean
    external fun archiveEntryCount(archiveHandle: Long): Int
    external fun archiveEntryName(archiveHandle: Long, index: Int): String?
    external fun archiveEntrySize(archiveHandle: Long, index: Int): Long
    external fun decodeArchiveEntry(
        sessionHandle: Long,
        requestId: Long,
        archiveHandle: Long,
        index: Int,
        mime: String?,
    ): Long
    external fun materializeArchiveEntry(
        sessionHandle: Long,
        requestId: Long,
        archiveHandle: Long,
        index: Int,
    ): Long
    external fun encodeRgba(
        sessionHandle: Long,
        requestId: Long,
        rgba: ByteBuffer,
        width: Int,
        height: Int,
        stride: Int,
        format: String,
    ): Long
    external fun bytesLength(bytesHandle: Long): Long
    external fun bytesBuffer(bytesHandle: Long): ByteBuffer?
    external fun releaseBytes(bytesHandle: Long): Boolean
}
