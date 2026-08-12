package io.github.mith_mmk.wml2viewer.platform.smb

internal class SharedCloseableCache<K, V : AutoCloseable>(
    private val factory: (K) -> V,
) : AutoCloseable {
    private val current = mutableMapOf<K, Entry<V>>()
    private var closed = false

    fun acquire(key: K): Lease<V> {
        val entry = synchronized(this) {
            check(!closed) { "Resource cache is closed" }
            current.getOrPut(key) { Entry(factory(key)) }.also { it.leases += 1 }
        }
        return Lease(entry.value) { release(entry) }
    }

    fun invalidateAll() {
        val closeNow = synchronized(this) { retireCurrent() }
        closeNow.forEach(::closeQuietly)
    }

    override fun close() {
        val closeNow = synchronized(this) {
            if (closed) return
            closed = true
            retireCurrent()
        }
        closeNow.forEach(::closeQuietly)
    }

    private fun retireCurrent(): List<V> = current.values
        .onEach { it.retired = true }
        .filter { it.leases == 0 }
        .map { it.value }
        .also { current.clear() }

    private fun release(entry: Entry<V>) {
        val closeNow = synchronized(this) {
            check(entry.leases > 0) { "Resource lease was already released" }
            entry.leases -= 1
            entry.value.takeIf { entry.retired && entry.leases == 0 }
        }
        closeNow?.let(::closeQuietly)
    }

    private fun closeQuietly(value: V) {
        runCatching { value.close() }
    }

    private class Entry<V>(val value: V, var leases: Int = 0, var retired: Boolean = false)

    class Lease<V> internal constructor(
        val value: V,
        private val release: () -> Unit,
    ) : AutoCloseable {
        private var closed = false

        override fun close() {
            synchronized(this) {
                if (closed) return
                closed = true
            }
            release()
        }
    }
}
