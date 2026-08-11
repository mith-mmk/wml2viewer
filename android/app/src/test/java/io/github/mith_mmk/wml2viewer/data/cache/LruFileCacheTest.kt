package io.github.mith_mmk.wml2viewer.data.cache

import com.google.common.truth.Truth.assertThat
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream

class LruFileCacheTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test
    fun leastRecentlyUsedUnpinnedEntryIsEvicted() {
        var clock = 1L
        val cache = LruFileCache(temporary.newFolder(), 6, 2, clock = { clock++ })
        cache.put("a", bytes("aaa")).close()
        cache.put("b", bytes("bbb")).close()
        cache.get("a")!!.close()
        cache.put("c", bytes("ccc")).close()
        cache.get("a")!!.close()
        assertThat(cache.get("b")).isNull()
        cache.get("c")!!.close()
        assertThat(cache.snapshot().entries).isEqualTo(2)
    }

    @Test
    fun pinnedEntriesAreNeverEvictedAndOversizeWriteLeavesNoEntry() {
        val cache = LruFileCache(temporary.newFolder(), 4, 1, maxSingleEntryBytes = 4)
        val pinned = cache.put("a", bytes("aaaa"))
        assertThrows(CacheLimitException::class.java) { cache.put("b", bytes("b")) }
        assertThat(cache.snapshot().entries).isEqualTo(1)
        pinned.close()
        cache.put("b", bytes("b")).close()
        assertThat(cache.get("a")).isNull()
        assertThrows(CacheLimitException::class.java) { cache.put("large", bytes("12345")) }
        assertThat(cache.get("large")).isNull()
    }

    @Test
    fun runtimeLimitUpdateTrimsAndRollsBackWhenPinned() {
        val cache = LruFileCache(temporary.newFolder(), 12, 3)
        cache.put("a", bytes("aaaa")).close()
        cache.put("b", bytes("bbbb")).close()
        cache.put("c", bytes("cccc")).close()
        cache.updateLimits(8, 2)
        assertThat(cache.snapshot().entries).isEqualTo(2)

        val pinned = cache.get("b") ?: cache.get("c")!!
        assertThrows(CacheLimitException::class.java) { cache.updateLimits(1, 1) }
        pinned.close()
        cache.put("still-valid", bytes("12345678")).close()
        assertThat(cache.snapshot().bytes).isAtMost(8)
    }

    private fun bytes(value: String) = ByteArrayInputStream(value.toByteArray())
}
