package io.github.mith_mmk.wml2viewer.platform.codec

import android.graphics.Bitmap
import io.github.mith_mmk.wml2viewer.nativebridge.NativeRequestError
import io.github.mith_mmk.wml2viewer.nativebridge.NativeSession
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * JNI-backed encoder used by INTERNAL codec routes. The native session stays
 * lazy so process startup and JVM/Robolectric tests never load the shared
 * library unless an internal encode is actually requested.
 */
class NativeRustCodecFallback : RustCodecFallback, AutoCloseable {
    private val sessionDelegate = lazy(LazyThreadSafetyMode.SYNCHRONIZED) { NativeSession() }
    private val session by sessionDelegate

    override val encodableFormats: Set<OsEncodeFormat> = setOf(
        OsEncodeFormat.PNG,
        OsEncodeFormat.WEBP_LOSSLESS,
    )

    override suspend fun decode(
        encoded: ByteArray,
        mimeType: String?,
        options: DecodeOptions,
    ): DecodedOsImage = throw UnsupportedCodecException(
        "Byte-array internal decoding requires a materialized source",
    )

    override suspend fun encode(bitmap: Bitmap, format: OsEncodeFormat, quality: Int): ByteArray =
        withContext(Dispatchers.Default) {
            // The current core API has no lossy quality parameter. Let the OS
            // backend handle formats for which silently ignoring quality would
            // change the user's requested result.
            val wireFormat = when (format) {
                OsEncodeFormat.PNG -> "png"
                OsEncodeFormat.WEBP_LOSSLESS -> "webp"
                OsEncodeFormat.JPEG,
                OsEncodeFormat.WEBP_LOSSY,
                -> throw UnsupportedCodecException("Lossy internal encoding is unavailable")
            }
            val rgba = ArgbToRgba.convert(bitmap)
            val encoded = session.encodeRgba(
                rgba = rgba,
                width = bitmap.width,
                height = bitmap.height,
                stride = Math.multiplyExact(bitmap.width, RGBA_BYTES_PER_PIXEL),
                format = wireFormat,
            ) ?: throw NativeCodecException(session.lastError)
            encoded.use { bytes ->
                if (bytes.length !in 1..MAX_ENCODED_BYTES) {
                    throw CodecLimitException("Internal encoder output exceeds the byte limit")
                }
                ByteArray(bytes.length.toInt()).also(bytes.readOnlyBuffer()::get)
            }
        }

    override fun close() {
        if (sessionDelegate.isInitialized()) session.close()
    }

    private companion object {
        const val RGBA_BYTES_PER_PIXEL = 4
        const val MAX_ENCODED_BYTES = 256L * 1024 * 1024
    }
}

/** Privacy-safe wrapper: UI consumes only [NativeRequestError.code] and scrubbed args. */
class NativeCodecException(
    val nativeError: NativeRequestError?,
) : CodecException("Internal codec operation failed")

internal object ArgbToRgba {
    private const val BYTES_PER_PIXEL = 4
    private const val MAX_PIXELS = 32_000_000L

    fun convert(bitmap: Bitmap): ByteBuffer {
        require(!bitmap.isRecycled) { "Bitmap is recycled" }
        val width = bitmap.width
        val height = bitmap.height
        val pixels = width.toLong() * height.toLong()
        if (width <= 0 || height <= 0 || pixels !in 1..MAX_PIXELS) {
            throw CodecLimitException("Bitmap exceeds the internal encoder pixel limit")
        }
        val byteCount = Math.multiplyExact(pixels, BYTES_PER_PIXEL.toLong()).toInt()
        val output = ByteBuffer.allocateDirect(byteCount)
        val row = IntArray(width)
        repeat(height) { y ->
            bitmap.getPixels(row, 0, width, 0, y, width, 1)
            appendRow(output, row, width)
        }
        return output.apply { flip() }
    }

    internal fun appendRow(output: ByteBuffer, argb: IntArray, count: Int = argb.size) {
        require(count in 0..argb.size) { "Invalid ARGB row length" }
        require(output.remaining() >= count * BYTES_PER_PIXEL) { "RGBA buffer is too small" }
        repeat(count) { index ->
            val pixel = argb[index]
            output.put((pixel ushr 16).toByte())
            output.put((pixel ushr 8).toByte())
            output.put(pixel.toByte())
            output.put((pixel ushr 24).toByte())
        }
    }
}
