package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import org.junit.Assert.assertThrows
import org.junit.Test

class SharedCloseableCacheTest {
    @Test
    fun repeatedAcquisitionsReuseResourceUntilInvalidated() {
        var created = 0
        val cache = SharedCloseableCache<String, TestResource> { TestResource(++created) }

        val first = cache.acquire("share")
        val second = cache.acquire("share")

        assertThat(second.value).isSameInstanceAs(first.value)
        first.close()
        second.close()
        assertThat(first.value.closed).isFalse()

        cache.invalidateAll()
        assertThat(first.value.closed).isTrue()
        cache.close()
    }

    @Test
    fun invalidationDefersCloseUntilOutstandingLeaseEnds() {
        var created = 0
        val cache = SharedCloseableCache<String, TestResource> { TestResource(++created) }
        val old = cache.acquire("share")

        cache.invalidateAll()
        val replacement = cache.acquire("share")

        assertThat(replacement.value.id).isEqualTo(2)
        assertThat(old.value.closed).isFalse()
        old.close()
        assertThat(old.value.closed).isTrue()
        replacement.close()
        cache.close()
        assertThat(replacement.value.closed).isTrue()
    }

    @Test
    fun closeRejectsNewAcquisitionsAndIsIdempotent() {
        val cache = SharedCloseableCache<String, TestResource> { TestResource(1) }
        val lease = cache.acquire("share")

        cache.close()
        cache.close()

        assertThat(lease.value.closed).isFalse()
        lease.close()
        assertThat(lease.value.closed).isTrue()
        assertThrows(IllegalStateException::class.java) { cache.acquire("share") }
    }

    private data class TestResource(val id: Int, var closed: Boolean = false) : AutoCloseable {
        override fun close() {
            closed = true
        }
    }
}
