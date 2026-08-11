package io.github.mith_mmk.wml2viewer.data.controller

import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import java.nio.charset.StandardCharsets
import java.util.Base64
import kotlin.math.min

internal object EntryUiTokenCodec {
    private val encoder = Base64.getUrlEncoder().withoutPadding()
    private val decoder = Base64.getUrlDecoder()

    fun encode(ref: EntryRef): String = "e1.${part(ref.providerId)}.${part(ref.opaqueId)}"

    fun decode(token: String): EntryRef {
        val parts = token.split('.', limit = 3)
        require(parts.size == 3 && parts[0] == "e1") { "Invalid entry token" }
        return EntryRef(text(parts[1]), text(parts[2]))
    }

    private fun part(value: String) = encoder.encodeToString(value.toByteArray(StandardCharsets.UTF_8))
    private fun text(value: String) = String(decoder.decode(value), StandardCharsets.UTF_8)
}

/** File-name order which keeps page2 before page10 without parsing integers. */
internal object NaturalFileNameComparator : Comparator<String> {
    override fun compare(left: String, right: String): Int {
        var leftIndex = 0
        var rightIndex = 0
        while (leftIndex < left.length && rightIndex < right.length) {
            val leftDigit = left[leftIndex].isDigit()
            val rightDigit = right[rightIndex].isDigit()
            if (leftDigit && rightDigit) {
                val leftEnd = left.scanRun(leftIndex, true)
                val rightEnd = right.scanRun(rightIndex, true)
                val runOrder = compareDigitRuns(
                    left.substring(leftIndex, leftEnd),
                    right.substring(rightIndex, rightEnd),
                )
                if (runOrder != 0) return runOrder
                leftIndex = leftEnd
                rightIndex = rightEnd
                continue
            }
            val leftEnd = left.scanRun(leftIndex, false)
            val rightEnd = right.scanRun(rightIndex, false)
            val runOrder = left.substring(leftIndex, leftEnd)
                .compareTo(right.substring(rightIndex, rightEnd), ignoreCase = true)
            if (runOrder != 0) return runOrder
            leftIndex = leftEnd
            rightIndex = rightEnd
        }
        return when {
            leftIndex < left.length -> 1
            rightIndex < right.length -> -1
            else -> left.compareTo(right)
        }
    }

    private fun String.scanRun(start: Int, digits: Boolean): Int {
        var index = start
        while (index < length && this[index].isDigit() == digits) index += 1
        return index
    }

    private fun compareDigitRuns(left: String, right: String): Int {
        val leftValue = left.trimStart('0').ifEmpty { "0" }
        val rightValue = right.trimStart('0').ifEmpty { "0" }
        if (leftValue.length != rightValue.length) return leftValue.length.compareTo(rightValue.length)
        val valueOrder = leftValue.compareTo(rightValue)
        return if (valueOrder != 0) valueOrder else left.length.compareTo(right.length)
    }
}

internal object MobileFileTypes {
    private val imageExtensions = setOf(
        "avif", "bmp", "dib", "dng", "gif", "heic", "heif", "ico", "jpe", "jpeg", "jpg",
        "mag", "mki", "pcd", "pi", "pic", "png", "tif", "tiff", "vsp", "webp",
    )
    private val archiveFormats = mapOf(
        "zip" to "zip",
        "cbz" to "zip",
        "lha" to "lha",
        "lzh" to "lzh",
        "wmltxt" to "wmltxt",
    )
    private val knownImageMimeTypes = setOf(
        "image/avif", "image/bmp", "image/dng", "image/x-adobe-dng", "image/gif",
        "image/heic", "image/heif", "image/x-icon", "image/vnd.microsoft.icon", "image/jpeg",
        "image/png", "image/svg+xml", "image/tiff", "image/webp",
    )

    fun isImage(name: String, mimeType: String?): Boolean =
        mimeType?.startsWith("image/", ignoreCase = true) == true || extension(name) in imageExtensions

    fun archiveFormat(name: String): String? = archiveFormats[extension(name)]

    fun mimeType(name: String, declared: String? = null): String? {
        val normalizedDeclared = declared
            ?.substringBefore(';')
            ?.trim()
            ?.lowercase()
            ?.takeIf { it in knownImageMimeTypes }
        if (normalizedDeclared != null) return normalizedDeclared
        return when (extension(name)) {
        "avif" -> "image/avif"
        "bmp", "dib" -> "image/bmp"
        "dng" -> "image/dng"
        "gif" -> "image/gif"
        "heic" -> "image/heic"
        "heif" -> "image/heif"
        "ico" -> "image/x-icon"
        "jpe", "jpeg", "jpg" -> "image/jpeg"
        "png" -> "image/png"
        "svg" -> "image/svg+xml"
        "tif", "tiff" -> "image/tiff"
        "webp" -> "image/webp"
        else -> null
        }
    }

    private fun extension(name: String): String = name.substringAfterLast('.', "").lowercase()
}

data class ComputedCacheLimit(
    val maxBytes: Long,
    val lowSpace: Boolean,
)

object MobileCacheLimitPolicy {
    const val MIB: Long = 1024L * 1024L
    const val GIB: Long = 1024L * MIB
    const val MINIMUM_BYTES: Long = 256L * MIB
    const val MAXIMUM_BYTES: Long = 2L * GIB
    const val MAX_SINGLE_ENTRY_BYTES: Long = 512L * MIB
    const val RESERVED_FREE_BYTES: Long = 1L * GIB

    /** Uses 10% of free space and never intentionally consumes the reserved final GiB. */
    fun automatic(usableBytes: Long): ComputedCacheLimit {
        require(usableBytes >= 0L) { "usableBytes must not be negative" }
        val desired = (usableBytes / 10L).coerceIn(MINIMUM_BYTES, MAXIMUM_BYTES)
        val reservable = (usableBytes - RESERVED_FREE_BYTES).coerceAtLeast(1L)
        return ComputedCacheLimit(min(desired, reservable), reservable < MINIMUM_BYTES)
    }

    fun manual(mebibytes: Int, usableBytes: Long): ComputedCacheLimit {
        val requested = mebibytes.coerceIn(256, 2_048).toLong() * MIB
        val reservable = (usableBytes - RESERVED_FREE_BYTES).coerceAtLeast(1L)
        return ComputedCacheLimit(min(requested, reservable), reservable < requested)
    }
}
