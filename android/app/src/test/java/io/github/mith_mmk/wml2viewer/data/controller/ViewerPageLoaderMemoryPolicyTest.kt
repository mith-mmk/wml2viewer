package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceCapabilities
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import org.junit.Test

class ViewerPageLoaderMemoryPolicyTest {
    @Test
    fun decodeLimitIncludesFourKSquareAndRejectsTheNextColumn() {
        assertThat(ViewerDecodeMemoryPolicy.MAX_PIXELS).isEqualTo(16_777_216L)
        assertThat(ViewerDecodeMemoryPolicy.requirePixelCount(4_096, 4_096))
            .isEqualTo(ViewerDecodeMemoryPolicy.MAX_PIXELS)
        assertThat(ViewerDecodeMemoryPolicy.rgbaBytes(4_096, 4_096))
            .isEqualTo(64L * 1024L * 1024L)
        assertThat(ViewerDecodeMemoryPolicy.OS_DECODE_OPTIONS.maxPixels)
            .isEqualTo(ViewerDecodeMemoryPolicy.MAX_PIXELS)
        assertThat(
            ViewerDecodeMemoryPolicy.MAX_COPY_TILE_PIXELS * Int.SIZE_BYTES,
        ).isEqualTo(1 * 1024 * 1024)

        val failure = runCatching {
            ViewerDecodeMemoryPolicy.requirePixelCount(4_097, 4_096)
        }.exceptionOrNull()
        assertThat(failure).isInstanceOf(IllegalArgumentException::class.java)
    }

    @Test
    fun multipleLargeImagesEvictByBytesInLruOrder() {
        val imageBytes = 64L * 1024L * 1024L
        val first = RecordingCloseable(imageBytes)
        val second = RecordingCloseable(imageBytes)
        val third = RecordingCloseable(imageBytes)
        val fourth = RecordingCloseable(imageBytes)
        val cache = cache()

        assertThat(cache.put("first", first)).isTrue()
        assertThat(cache.put("second", second)).isTrue()
        assertThat(cache.put("third", third)).isTrue()
        assertThat(first.closeCount).isEqualTo(1)
        assertThat(cache.retainedWeight).isEqualTo(ViewerDecodeMemoryPolicy.MAX_FRAME_CACHE_BYTES)

        assertThat(cache["second"]).isSameInstanceAs(second)
        assertThat(cache.put("fourth", fourth)).isTrue()
        assertThat(third.closeCount).isEqualTo(1)
        assertThat(second.closeCount).isEqualTo(0)
        assertThat(cache.size).isEqualTo(2)

        cache.close()
        assertThat(second.closeCount).isEqualTo(1)
        assertThat(fourth.closeCount).isEqualTo(1)
        assertThat(cache.retainedWeight).isEqualTo(0L)
    }

    @Test
    fun oversizedValueIsNotOwnedAndReplacementClosesExactlyOnce() {
        val cache = cache()
        val original = RecordingCloseable(1L)
        val replacement = RecordingCloseable(1L)
        val oversized = RecordingCloseable(ViewerDecodeMemoryPolicy.MAX_FRAME_CACHE_BYTES + 1L)

        assertThat(cache.put("page", original)).isTrue()
        assertThat(cache.put("page", replacement)).isTrue()
        assertThat(original.closeCount).isEqualTo(1)
        assertThat(cache.put("oversized", oversized)).isFalse()
        assertThat(oversized.closeCount).isEqualTo(0)

        cache.clear()
        cache.clear()
        assertThat(replacement.closeCount).isEqualTo(1)
        oversized.close()
        assertThat(oversized.closeCount).isEqualTo(1)
    }

    @Test
    fun loadedPageCountsNativeRgbaAndClosesItsSourceOnce() {
        val bitmap = TestImageBitmap(10, 10)
        val source = RecordingAnimationSource(bitmap, retainedRgbaBytes = 1_000L)
        val loaded = LoadedViewerPage(
            page = page(),
            frame = bitmap,
            portrait = true,
            animationSource = source,
        )

        assertThat(loaded.estimatedRetainedBytes()).isEqualTo(1_400L)
        loaded.close()
        loaded.close()
        assertThat(source.closeCount).isEqualTo(1)
    }

    private fun cache() = CloseableWeightedLruCache<String, RecordingCloseable>(
        maxEntries = 6,
        maxWeight = ViewerDecodeMemoryPolicy.MAX_FRAME_CACHE_BYTES,
        weigh = RecordingCloseable::bytes,
    )

    private fun page() = ViewerPageSource.Direct(
        SourceEntry(
            ref = EntryRef("test", "page"),
            parent = null,
            name = "page.png",
            kind = EntryKind.FILE,
            mimeType = "image/png",
            size = null,
            modifiedAtEpochMillis = null,
            isHidden = false,
            effectiveCapabilities = SourceCapabilities(),
        ),
    )

    private class RecordingCloseable(val bytes: Long) : AutoCloseable {
        var closeCount = 0
            private set

        override fun close() {
            closeCount += 1
        }
    }

    private class RecordingAnimationSource(
        private val bitmap: TestImageBitmap,
        override val retainedRgbaBytes: Long,
    ) : LoadedAnimationSource {
        var closeCount = 0
            private set

        override val frameCount: Int = 2

        override fun frame(index: Int): LoadedAnimationFrame {
            require(index in 0 until frameCount)
            return LoadedAnimationFrame(bitmap, 100L)
        }

        override fun close() {
            closeCount += 1
        }
    }
}
