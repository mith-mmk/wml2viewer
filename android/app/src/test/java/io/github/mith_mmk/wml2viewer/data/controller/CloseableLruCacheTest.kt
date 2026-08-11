package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class CloseableLruCacheTest {
    @Test
    fun evictionAndClearCloseEachOwnedValue() {
        val first = RecordingCloseable()
        val second = RecordingCloseable()
        val cache = CloseableLruCache<String, RecordingCloseable>(1)

        cache.put("first", first)
        cache.put("second", second)
        assertThat(first.closeCount).isEqualTo(1)
        assertThat(second.closeCount).isEqualTo(0)

        cache.clear()
        assertThat(second.closeCount).isEqualTo(1)
    }

    @Test
    fun predicateRemovalClosesOnlyMatchingValues() {
        val keep = RecordingCloseable()
        val remove = RecordingCloseable()
        val cache = CloseableLruCache<String, RecordingCloseable>(2)
        cache.put("keep", keep)
        cache.put("remove", remove)

        cache.removeIf { key, _ -> key == "remove" }

        assertThat(keep.closeCount).isEqualTo(0)
        assertThat(remove.closeCount).isEqualTo(1)
        cache.close()
        assertThat(keep.closeCount).isEqualTo(1)
    }

    private class RecordingCloseable : AutoCloseable {
        var closeCount = 0
        override fun close() {
            closeCount++
        }
    }
}
