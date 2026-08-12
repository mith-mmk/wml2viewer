package io.github.mith_mmk.wml2viewer.nativebridge

enum class NativeReadingLayout { AUTO, SINGLE, SPREAD }

enum class NativeReadingDirection { LEFT_TO_RIGHT, RIGHT_TO_LEFT }

data class NativeReadingPage(
    val sourceId: Long,
    val portrait: Boolean,
    val cover: Boolean = false,
)

data class NativeReadingPlan(
    val anchorIndex: Int,
    val logicalIndices: List<Int>,
    val visualIndices: List<Int>,
    val previousAnchorIndex: Int?,
    val nextAnchorIndex: Int?,
    val preloadIndices: List<Int>,
)

/** Typed Kotlin boundary for the platform-independent Rust reading model. */
object NativeReadingPlanner {
    const val MAX_PAGES = 4_096
    const val MAX_PREFETCH_SPREADS = 64

    internal const val WIRE_VERSION = 1
    private const val WIRE_HEADER_INTS = 8
    private const val WIRE_NONE = -1

    fun plan(
        pages: List<NativeReadingPage>,
        currentIndex: Int,
        isLandscape: Boolean,
        layout: NativeReadingLayout = NativeReadingLayout.AUTO,
        direction: NativeReadingDirection = NativeReadingDirection.RIGHT_TO_LEFT,
        coverAlone: Boolean = true,
        maxPrefetchSpreads: Int = 1,
    ): NativeReadingPlan? {
        validateInputBounds(pages.size, currentIndex, maxPrefetchSpreads)
        val sourceIds = LongArray(pages.size)
        val portrait = BooleanArray(pages.size)
        val covers = BooleanArray(pages.size)
        pages.forEachIndexed { index, page ->
            sourceIds[index] = page.sourceId
            portrait[index] = page.portrait
            covers[index] = page.cover
        }
        val wire = NativeBridge.planReading(
            sourceIds = sourceIds,
            portrait = portrait,
            covers = covers,
            currentIndex = currentIndex,
            landscape = isLandscape,
            layout = layout.toWire(),
            direction = direction.toWire(),
            coverAlone = coverAlone,
            maxPrefetchSpreads = maxPrefetchSpreads,
        ) ?: return null
        return decodeWire(wire, pages.size, currentIndex, maxPrefetchSpreads)
    }

    internal fun validateInputBounds(
        pageCount: Int,
        currentIndex: Int,
        maxPrefetchSpreads: Int,
    ) {
        require(pageCount in 1..MAX_PAGES) {
            "page count must be between 1 and $MAX_PAGES"
        }
        require(currentIndex in 0 until pageCount) { "current index is outside the page list" }
        require(maxPrefetchSpreads in 0..MAX_PREFETCH_SPREADS) {
            "prefetch spread count must be between 0 and $MAX_PREFETCH_SPREADS"
        }
    }

    /**
     * Wire v1 fields: version, total length, anchor, previous anchor, next anchor,
     * logical count, visual count, preload count, then those three index lists.
     */
    internal fun decodeWire(
        wire: IntArray,
        pageCount: Int,
        currentIndex: Int,
        maxPrefetchSpreads: Int,
    ): NativeReadingPlan? {
        if (pageCount !in 1..MAX_PAGES || currentIndex !in 0 until pageCount) return null
        if (maxPrefetchSpreads !in 0..MAX_PREFETCH_SPREADS) return null
        if (wire.size < WIRE_HEADER_INTS || wire[0] != WIRE_VERSION || wire[1] != wire.size) {
            return null
        }
        val anchor = wire[2]
        val previous = wire[3]
        val next = wire[4]
        val logicalCount = wire[5]
        val visualCount = wire[6]
        val preloadCount = wire[7]
        if (logicalCount !in 1..2 || visualCount != logicalCount || preloadCount < 0) return null
        if (preloadCount > maxPrefetchSpreads * 2) return null
        val expectedSize = WIRE_HEADER_INTS.toLong() +
            logicalCount.toLong() + visualCount.toLong() + preloadCount.toLong()
        if (expectedSize != wire.size.toLong()) return null
        if (anchor !in 0 until pageCount) return null
        if (previous != WIRE_NONE && previous !in 0 until pageCount) return null
        if (next != WIRE_NONE && next !in 0 until pageCount) return null

        var cursor = WIRE_HEADER_INTS
        fun readIndices(count: Int): List<Int> {
            val indices = wire.copyOfRange(cursor, cursor + count).toList()
            cursor += count
            return indices
        }
        val logical = readIndices(logicalCount)
        val visual = readIndices(visualCount)
        val preload = readIndices(preloadCount)
        if ((logical + visual + preload).any { it !in 0 until pageCount }) return null
        if (logical.first() != anchor || currentIndex !in logical) return null
        if (logical.toSet().size != logical.size || visual.toSet() != logical.toSet()) return null

        return NativeReadingPlan(
            anchorIndex = anchor,
            logicalIndices = logical,
            visualIndices = visual,
            previousAnchorIndex = previous.takeUnless { it == WIRE_NONE },
            nextAnchorIndex = next.takeUnless { it == WIRE_NONE },
            preloadIndices = preload,
        )
    }

    private fun NativeReadingLayout.toWire(): Int = when (this) {
        NativeReadingLayout.AUTO -> 0
        NativeReadingLayout.SINGLE -> 1
        NativeReadingLayout.SPREAD -> 2
    }

    private fun NativeReadingDirection.toWire(): Int = when (this) {
        NativeReadingDirection.LEFT_TO_RIGHT -> 0
        NativeReadingDirection.RIGHT_TO_LEFT -> 1
    }
}
