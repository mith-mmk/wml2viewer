package io.github.mith_mmk.wml2viewer.nativebridge

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeReadingPlannerInstrumentationTest {
    @Test
    fun coverAndPortraitPagesUseRustRtlSpreadAndPreloadPlan() {
        val pages = (0 until 6).map { index ->
            NativeReadingPage(sourceId = 7, portrait = true, cover = index == 0)
        }

        val cover = NativeReadingPlanner.plan(pages, 0, true)!!
        val spread = NativeReadingPlanner.plan(pages, 1, true, maxPrefetchSpreads = 1)!!

        assertEquals(listOf(0), cover.logicalIndices)
        assertEquals(1, cover.nextAnchorIndex)
        assertEquals(listOf(1, 2), spread.logicalIndices)
        assertEquals(listOf(2, 1), spread.visualIndices)
        assertEquals(0, spread.previousAnchorIndex)
        assertEquals(3, spread.nextAnchorIndex)
        assertEquals(listOf(3, 4), spread.preloadIndices)
    }

    @Test
    fun viewportOrientationAndSourceBoundaryAreOwnedByRustPlanner() {
        val pages = listOf(
            NativeReadingPage(1, portrait = true, cover = true),
            NativeReadingPage(1, portrait = true),
            NativeReadingPage(2, portrait = true, cover = true),
        )

        val portrait = NativeReadingPlanner.plan(pages, 1, false)!!
        val boundary = NativeReadingPlanner.plan(pages, 2, true)!!
        val forced = NativeReadingPlanner.plan(
            pages = pages,
            currentIndex = 1,
            isLandscape = false,
            layout = NativeReadingLayout.SPREAD,
            direction = NativeReadingDirection.LEFT_TO_RIGHT,
        )!!

        assertEquals(listOf(1), portrait.logicalIndices)
        assertEquals(listOf(2), boundary.logicalIndices)
        assertEquals(listOf(1), forced.logicalIndices)
        assertEquals(forced.logicalIndices, forced.visualIndices)
        assertNull(boundary.nextAnchorIndex)
        assertTrue(boundary.preloadIndices.isEmpty())
    }
}
