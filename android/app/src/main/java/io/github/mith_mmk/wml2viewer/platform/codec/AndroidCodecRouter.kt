package io.github.mith_mmk.wml2viewer.platform.codec

import android.content.ContentResolver
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import androidx.annotation.RequiresApi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlin.math.roundToInt

enum class OsEncodeFormat(val mimeType: String) {
    PNG("image/png"),
    JPEG("image/jpeg"),
    WEBP_LOSSY("image/webp"),
    WEBP_LOSSLESS("image/webp"),
}

enum class CodecFormat {
    JPEG, PNG, GIF, WEBP, BMP, ICO, HEIF, AVIF, DNG, UNKNOWN;

    companion object {
        fun fromMimeType(mimeType: String?): CodecFormat = when (mimeType?.lowercase()) {
            "image/jpeg", "image/jpg" -> JPEG
            "image/png" -> PNG
            "image/gif" -> GIF
            "image/webp" -> WEBP
            "image/bmp", "image/x-ms-bmp" -> BMP
            "image/x-icon", "image/vnd.microsoft.icon" -> ICO
            "image/heif", "image/heic" -> HEIF
            "image/avif" -> AVIF
            "image/dng", "image/x-adobe-dng" -> DNG
            else -> UNKNOWN
        }
    }
}

enum class CodecRoute { DEFAULT, INTERNAL_FIRST, OS_FIRST, INTERNAL_ONLY, OS_ONLY }
enum class CodecBackend { INTERNAL, OS }

data class CodecRoutePolicy(
    val global: CodecRoute = CodecRoute.INTERNAL_FIRST,
    val overrides: Map<CodecFormat, CodecRoute> = emptyMap(),
) {
    init {
        require(global != CodecRoute.DEFAULT) { "Global codec route cannot be DEFAULT" }
    }

    fun routeFor(format: CodecFormat): CodecRoute = overrides[format]
        ?.takeUnless { it == CodecRoute.DEFAULT }
        ?: global

    fun orderFor(format: CodecFormat): List<CodecBackend> = when (routeFor(format)) {
        CodecRoute.INTERNAL_FIRST -> listOf(CodecBackend.INTERNAL, CodecBackend.OS)
        CodecRoute.OS_FIRST -> listOf(CodecBackend.OS, CodecBackend.INTERNAL)
        CodecRoute.INTERNAL_ONLY -> listOf(CodecBackend.INTERNAL)
        CodecRoute.OS_ONLY -> listOf(CodecBackend.OS)
        CodecRoute.DEFAULT -> error("DEFAULT must be resolved")
    }
}

internal object CodecRouteExecutor {
    suspend fun <T> execute(
        order: List<CodecBackend>,
        internal: suspend () -> T,
        os: suspend () -> T,
        isTerminal: (Throwable) -> Boolean = { false },
    ): T {
        val failures = ArrayList<Throwable>()
        for (backend in order) {
            try {
                return when (backend) {
                    CodecBackend.INTERNAL -> internal()
                    CodecBackend.OS -> os()
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                if (isTerminal(error)) throw error
                failures += error
            }
        }
        throw CodecRouteException("All permitted codec backends failed", failures)
    }
}

data class CodecCapabilities(
    val decodableMimeTypes: Set<String>,
    val decodableFormats: Set<CodecFormat>,
    val encodableFormats: Set<OsEncodeFormat>,
    val supportsAnimatedDrawable: Boolean,
)

data class DecodeOptions(
    val maxWidth: Int = 16_384,
    val maxHeight: Int = 16_384,
    val maxPixels: Long = 100_000_000,
    val maxEncodedBytes: Long = 512L * 1024 * 1024,
) {
    init {
        require(maxWidth > 0 && maxHeight > 0 && maxPixels > 0 && maxEncodedBytes > 0) { "Invalid decode limits" }
    }
}

data class DecodedOsImage(
    val bitmap: Bitmap,
    val mimeType: String,
    val sourceWidth: Int,
    val sourceHeight: Int,
    val animatedSource: Boolean,
) : AutoCloseable {
    /** Android ARGB_8888 native byte order; consumers must not assume RGBA ordering. */
    fun toArgb8888DirectBuffer(): ByteBuffer {
        val software = if (bitmap.config == Bitmap.Config.ARGB_8888) bitmap else bitmap.copy(Bitmap.Config.ARGB_8888, false)
        return try {
            ByteBuffer.allocateDirect(software.byteCount).also { buffer ->
                software.copyPixelsToBuffer(buffer)
                buffer.flip()
            }
        } finally {
            if (software !== bitmap) software.recycle()
        }
    }

    override fun close() {
        bitmap.recycle()
    }
}

interface RustCodecFallback {
    val encodableFormats: Set<OsEncodeFormat>
        get() = emptySet()

    suspend fun decode(encoded: ByteArray, mimeType: String?, options: DecodeOptions): DecodedOsImage
    suspend fun encode(bitmap: Bitmap, format: OsEncodeFormat, quality: Int): ByteArray
}

class AndroidCodecCapabilityProbe {
    private val measured: CodecCapabilities by lazy(::measure)

    fun probe(): CodecCapabilities = measured

    private fun measure(): CodecCapabilities {
        val successfulEncoders = linkedSetOf<OsEncodeFormat>()
        val successfulDecoders = linkedSetOf<CodecFormat>()
        val sample = Bitmap.createBitmap(2, 2, Bitmap.Config.ARGB_8888).apply {
            setPixels(intArrayOf(0xFFFF0000.toInt(), 0xFF00FF00.toInt(), 0xFF0000FF.toInt(), 0xFFFFFFFF.toInt()), 0, 2, 0, 0, 2, 2)
        }
        try {
            candidateEncoders().forEach { format ->
                if (encodeRoundTrip(sample, format) != null) {
                    successfulEncoders += format
                    successfulDecoders += format.codecFormat
                }
            }
            AndroidCodecProbeFixtures.encoded.forEach { (format, encoded) ->
                if (decodeRoundTrip(encoded)) successfulDecoders += format
            }
        } finally {
            sample.recycle()
        }
        val decodableMimeTypes = successfulDecoders.flatMapTo(linkedSetOf<String>()) { format ->
            mimeTypesFor(format)
        }
        return CodecCapabilities(
            decodableMimeTypes = decodableMimeTypes,
            decodableFormats = successfulDecoders,
            encodableFormats = successfulEncoders,
            // ImageDecoder has no public deterministic frame extraction API.
            supportsAnimatedDrawable = false,
        )
    }

    fun canDecode(mimeType: String?): Boolean =
        mimeType != null && mimeType.lowercase() in measured.decodableMimeTypes

    private fun candidateEncoders(): Set<OsEncodeFormat> = buildSet {
        add(OsEncodeFormat.PNG)
        add(OsEncodeFormat.JPEG)
        add(OsEncodeFormat.WEBP_LOSSY)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) add(OsEncodeFormat.WEBP_LOSSLESS)
    }

    private fun encodeRoundTrip(bitmap: Bitmap, format: OsEncodeFormat): ByteArray? = runCatching {
        val bytes = ByteArrayOutputStream()
        val compressFormat = when (format) {
            OsEncodeFormat.PNG -> Bitmap.CompressFormat.PNG
            OsEncodeFormat.JPEG -> Bitmap.CompressFormat.JPEG
            OsEncodeFormat.WEBP_LOSSY -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                Bitmap.CompressFormat.WEBP_LOSSY
            } else {
                @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
            }
            OsEncodeFormat.WEBP_LOSSLESS -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                webpLosslessFormat()
            } else {
                throw UnsupportedCodecException("Lossless WebP encoding requires Android 11")
            }
        }
        check(bitmap.compress(compressFormat, 90, bytes))
        val encoded = bytes.toByteArray()
        val decoded = decodeBitmap(encoded)
        try {
            encoded.takeIf { decoded.width == bitmap.width && decoded.height == bitmap.height }
        } finally {
            decoded.recycle()
        }
    }.getOrNull()

    private fun decodeRoundTrip(encoded: ByteArray): Boolean = runCatching {
        val decoded = decodeBitmap(encoded)
        try {
            if (decoded.width <= 0 || decoded.height <= 0) return@runCatching false
            val png = ByteArrayOutputStream()
            check(decoded.compress(Bitmap.CompressFormat.PNG, 100, png))
            val roundTripped = decodeBitmap(png.toByteArray())
            try {
                roundTripped.width == decoded.width && roundTripped.height == decoded.height
            } finally {
                roundTripped.recycle()
            }
        } finally {
            decoded.recycle()
        }
    }.getOrDefault(false)

    private fun decodeBitmap(encoded: ByteArray): Bitmap = ImageDecoder.decodeBitmap(
        ImageDecoder.createSource(ByteBuffer.wrap(encoded)),
    ) { decoder, _, _ ->
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
    }

    @RequiresApi(Build.VERSION_CODES.R)
    private fun webpLosslessFormat(): Bitmap.CompressFormat = Bitmap.CompressFormat.WEBP_LOSSLESS

    companion object {
        val CANDIDATE_MIME_TYPES = setOf(
            "image/jpeg",
            "image/png",
            "image/gif",
            "image/webp",
            "image/bmp",
            "image/x-icon",
            "image/heif",
            "image/heic",
            "image/avif",
            "image/dng",
        )

        private fun mimeTypesFor(format: CodecFormat): Set<String> = when (format) {
            CodecFormat.JPEG -> setOf("image/jpeg", "image/jpg")
            CodecFormat.PNG -> setOf("image/png")
            CodecFormat.GIF -> setOf("image/gif")
            CodecFormat.WEBP -> setOf("image/webp")
            CodecFormat.BMP -> setOf("image/bmp", "image/x-ms-bmp")
            CodecFormat.ICO -> setOf("image/x-icon", "image/vnd.microsoft.icon")
            CodecFormat.HEIF -> setOf("image/heif", "image/heic")
            CodecFormat.AVIF -> setOf("image/avif")
            CodecFormat.DNG -> setOf("image/dng", "image/x-adobe-dng")
            CodecFormat.UNKNOWN -> emptySet()
        }
    }
}

private val OsEncodeFormat.codecFormat: CodecFormat
    get() = when (this) {
        OsEncodeFormat.PNG -> CodecFormat.PNG
        OsEncodeFormat.JPEG -> CodecFormat.JPEG
        OsEncodeFormat.WEBP_LOSSY,
        OsEncodeFormat.WEBP_LOSSLESS,
        -> CodecFormat.WEBP
    }

internal fun availableEncodeFormatsForPolicy(
    policy: CodecRoutePolicy,
    osFormats: Set<OsEncodeFormat>,
    internalFormats: Set<OsEncodeFormat>,
): Set<OsEncodeFormat> = OsEncodeFormat.entries.filterTo(linkedSetOf()) { format ->
    policy.orderFor(format.codecFormat).any { backend ->
        when (backend) {
            CodecBackend.OS -> format in osFormats
            CodecBackend.INTERNAL -> format in internalFormats
        }
    }
}

/** Selects the device codec at runtime and delegates unsupported formats to the Rust core. */
class AndroidCodecRouter(
    private val fallback: RustCodecFallback? = null,
    private val probe: AndroidCodecCapabilityProbe = AndroidCodecCapabilityProbe(),
    routePolicy: CodecRoutePolicy = CodecRoutePolicy(),
) {
    private val routePolicy = AtomicReference(routePolicy)
    val capabilities: CodecCapabilities by lazy(probe::probe)
    val availableEncodeFormats: Set<OsEncodeFormat>
        get() = availableEncodeFormats(routePolicy.get())

    fun availableEncodeFormats(policy: CodecRoutePolicy): Set<OsEncodeFormat> =
        availableEncodeFormatsForPolicy(
            policy = policy,
            osFormats = capabilities.encodableFormats,
            internalFormats = fallback?.encodableFormats ?: emptySet(),
        )

    fun currentPolicy(): CodecRoutePolicy = routePolicy.get()

    /** A settings update affects the next operation; each in-flight operation uses one policy snapshot. */
    fun updatePolicy(policy: CodecRoutePolicy) {
        routePolicy.set(policy)
    }

    fun withPolicy(policy: CodecRoutePolicy): AndroidCodecRouter = AndroidCodecRouter(fallback, probe, policy)

    suspend fun decode(
        encoded: ByteArray,
        mimeType: String? = null,
        options: DecodeOptions = DecodeOptions(),
    ): DecodedOsImage = decodeWithPolicy(encoded, mimeType, options, routePolicy.get())

    suspend fun decodeWithPolicy(
        encoded: ByteArray,
        mimeType: String? = null,
        options: DecodeOptions = DecodeOptions(),
        policy: CodecRoutePolicy,
    ): DecodedOsImage {
        require(encoded.isNotEmpty()) { "Encoded image must not be empty" }
        require(encoded.size.toLong() <= options.maxEncodedBytes) { "Encoded image exceeds the configured byte limit" }
        return CodecRouteExecutor.execute(
            order = policy.orderFor(CodecFormat.fromMimeType(mimeType)),
            internal = {
                fallback?.decode(encoded, mimeType, options)
                    ?: throw UnsupportedCodecException("Internal codec is unavailable")
            },
            os = {
                withContext(Dispatchers.IO) {
                    decodeSource(ImageDecoder.createSource(ByteBuffer.wrap(encoded)), options)
                }
            },
            isTerminal = { it is CodecLimitException },
        )
    }

    /** OS decode path for a materialized cache file; no readBytes() copy is made. */
    suspend fun decode(
        file: File,
        mimeType: String? = null,
        options: DecodeOptions = DecodeOptions(),
    ): DecodedOsImage {
        require(file.isFile) { "Encoded image file is unavailable" }
        val length = file.length()
        require(length in 1..options.maxEncodedBytes) { "Encoded image exceeds the configured byte limit" }
        return withContext(Dispatchers.IO) {
            decodeSource(ImageDecoder.createSource(file), options)
        }
    }

    /** OS decode path for NativeBytes/direct buffers; the caller's position is not changed. */
    suspend fun decode(
        encoded: ByteBuffer,
        mimeType: String? = null,
        options: DecodeOptions = DecodeOptions(),
    ): DecodedOsImage {
        val remaining = encoded.remaining().toLong()
        require(remaining in 1..options.maxEncodedBytes) { "Encoded image exceeds the configured byte limit" }
        val source = encoded.slice()
        return withContext(Dispatchers.IO) {
            decodeSource(ImageDecoder.createSource(source), options)
        }
    }

    suspend fun decode(
        resolver: ContentResolver,
        uri: Uri,
        options: DecodeOptions = DecodeOptions(),
    ): DecodedOsImage = withContext(Dispatchers.IO) {
        try {
            decodeSource(ImageDecoder.createSource(resolver, uri), options)
        } catch (error: IOException) {
            throw CodecException("OS image decode failed", error)
        }
    }

    suspend fun encode(bitmap: Bitmap, format: OsEncodeFormat, quality: Int = 90): ByteArray {
        require(quality in 0..100) { "quality must be between 0 and 100" }
        return CodecRouteExecutor.execute(
            order = routePolicy.get().orderFor(format.codecFormat),
            internal = {
                fallback?.encode(bitmap, format, quality)
                    ?: throw UnsupportedCodecException("Internal codec is unavailable")
            },
            os = { encodeWithOs(bitmap, format, quality) },
        )
    }

    private suspend fun encodeWithOs(bitmap: Bitmap, format: OsEncodeFormat, quality: Int): ByteArray {
        if (format !in capabilities.encodableFormats) throw UnsupportedCodecException("OS encoder probe did not pass")
        return withContext(Dispatchers.Default) {
            val bytes = ByteArrayOutputStream()
            val androidFormat = when (format) {
                OsEncodeFormat.PNG -> Bitmap.CompressFormat.PNG
                OsEncodeFormat.JPEG -> Bitmap.CompressFormat.JPEG
                OsEncodeFormat.WEBP_LOSSY -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    Bitmap.CompressFormat.WEBP_LOSSY
                } else {
                    @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
                }
                OsEncodeFormat.WEBP_LOSSLESS -> {
                    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                        throw UnsupportedCodecException("Lossless WebP encoding requires Android 11")
                    }
                    Bitmap.CompressFormat.WEBP_LOSSLESS
                }
            }
            if (!bitmap.compress(androidFormat, quality, bytes)) throw CodecException("OS image encode failed")
            bytes.toByteArray()
        }
    }

    private fun decodeSource(source: ImageDecoder.Source, options: DecodeOptions): DecodedOsImage {
        var headerMime = "application/octet-stream"
        var sourceWidth = 0
        var sourceHeight = 0
        var animated = false
        val bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            sourceWidth = info.size.width
            sourceHeight = info.size.height
            headerMime = info.mimeType
            animated = info.isAnimated
            val pixels = sourceWidth.toLong() * sourceHeight.toLong()
            if (pixels <= 0 || pixels > options.maxPixels) {
                throw CodecLimitException("Image exceeds the configured pixel limit")
            }
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.isMutableRequired = false
            decoder.setOnPartialImageListener { false }
            val scale = minOf(
                1.0,
                options.maxWidth.toDouble() / sourceWidth,
                options.maxHeight.toDouble() / sourceHeight,
            )
            if (scale < 1.0) {
                decoder.setTargetSize(
                    (sourceWidth * scale).roundToInt().coerceAtLeast(1),
                    (sourceHeight * scale).roundToInt().coerceAtLeast(1),
                )
            }
        }
        return DecodedOsImage(bitmap, headerMime, sourceWidth, sourceHeight, animated)
    }
}

open class CodecException(message: String, cause: Throwable? = null) : Exception(message, cause)
class UnsupportedCodecException(message: String) : CodecException(message)
class CodecLimitException(message: String) : CodecException(message)
class CodecRouteException(message: String, val failures: List<Throwable>) : CodecException(
    message,
    failures.lastOrNull(),
)
