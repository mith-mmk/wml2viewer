package io.github.mith_mmk.wml2viewer.data.controller

/** Small access-ordered cache that deterministically closes every evicted value. */
internal class CloseableLruCache<K, V : AutoCloseable>(
    private val maxEntries: Int,
) : AutoCloseable {
    private val entries = LinkedHashMap<K, V>(maxEntries + 1, 0.75f, true)

    init {
        require(maxEntries > 0) { "maxEntries must be positive" }
    }

    @Synchronized
    operator fun get(key: K): V? = entries[key]

    @Synchronized
    fun put(key: K, value: V) {
        val previous = entries.put(key, value)
        if (previous != null && previous !== value) previous.close()
        while (entries.size > maxEntries) {
            val iterator = entries.entries.iterator()
            val eldest = iterator.next()
            iterator.remove()
            if (eldest.value !== value) eldest.value.close()
        }
    }

    @Synchronized
    fun removeIf(predicate: (K, V) -> Boolean) {
        val iterator = entries.entries.iterator()
        while (iterator.hasNext()) {
            val entry = iterator.next()
            if (predicate(entry.key, entry.value)) {
                iterator.remove()
                entry.value.close()
            }
        }
    }

    @Synchronized
    fun clear() {
        val values = entries.values.toList()
        entries.clear()
        values.forEach(AutoCloseable::close)
    }

    override fun close() = clear()
}
