package io.github.mith_mmk.wml2viewer.nativebridge

import com.google.common.truth.Truth.assertThat
import org.junit.Assert.assertThrows
import org.junit.Test

class NativeReadingPlannerTest {
    @Test
    fun versionedWireDecodesToTypedIndicesAndAnchors() {
        val plan = NativeReadingPlanner.decodeWire(
            wire = intArrayOf(
                NativeReadingPlanner.WIRE_VERSION,
                15,
                1,
                0,
                3,
                2,
                2,
                3,
                1,
                2,
                2,
                1,
                3,
                4,
                5,
            ),
            pageCount = 6,
            currentIndex = 2,
            maxPrefetchSpreads = 2,
        )

        assertThat(plan).isEqualTo(
            NativeReadingPlan(
                anchorIndex = 1,
                logicalIndices = listOf(1, 2),
                visualIndices = listOf(2, 1),
                previousAnchorIndex = 0,
                nextAnchorIndex = 3,
                preloadIndices = listOf(3, 4, 5),
            ),
        )
    }

    @Test
    fun malformedOrOutOfBoundsWireIsRejected() {
        val valid = intArrayOf(1, 10, 0, -1, 1, 1, 1, 0, 0, 0)

        assertThat(
            NativeReadingPlanner.decodeWire(
                valid.copyOf().also { it[0] = 2 },
                2,
                0,
                1,
            ),
        ).isNull()
        assertThat(
            NativeReadingPlanner.decodeWire(
                valid.copyOf().also { it[1] = 9 },
                2,
                0,
                1,
            ),
        ).isNull()
        assertThat(
            NativeReadingPlanner.decodeWire(
                valid.copyOf().also { it[8] = 2 },
                2,
                0,
                1,
            ),
        ).isNull()
        assertThat(
            NativeReadingPlanner.decodeWire(
                intArrayOf(1, 11, 0, -1, 1, 1, 1, 1, 0, 0, 1),
                2,
                0,
                0,
            ),
        ).isNull()
    }

    @Test
    fun inputValidationCapsPagesCurrentIndexAndPrefetch() {
        NativeReadingPlanner.validateInputBounds(
            pageCount = NativeReadingPlanner.MAX_PAGES,
            currentIndex = NativeReadingPlanner.MAX_PAGES - 1,
            maxPrefetchSpreads = NativeReadingPlanner.MAX_PREFETCH_SPREADS,
        )

        assertThrows(IllegalArgumentException::class.java) {
            NativeReadingPlanner.validateInputBounds(0, 0, 1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            NativeReadingPlanner.validateInputBounds(NativeReadingPlanner.MAX_PAGES + 1, 0, 1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            NativeReadingPlanner.validateInputBounds(1, 1, 1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            NativeReadingPlanner.validateInputBounds(
                1,
                0,
                NativeReadingPlanner.MAX_PREFETCH_SPREADS + 1,
            )
        }
    }
}
