package io.github.mith_mmk.wml2viewer.ui.touch

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class GesturePriorityTest {
    @Test
    fun higherPriorityReplacesLowerAndCannotBeDowngraded() {
        val arbiter = GestureArbiter()
        arbiter.beginPointerSequence()

        assertThat(arbiter.accept(GestureIntent.SINGLE_TAP)).isTrue()
        assertThat(arbiter.accept(GestureIntent.LONG_PRESS)).isTrue()
        assertThat(arbiter.accept(GestureIntent.PINCH)).isTrue()
        assertThat(arbiter.accept(GestureIntent.PAN)).isFalse()
        assertThat(arbiter.current()).isEqualTo(GestureIntent.PINCH)
    }

    @Test
    fun priorityIsPinchPanSwipeLongDoubleSingle() {
        assertThat(GestureIntent.entries.sortedByDescending { it.priority }).containsExactly(
            GestureIntent.PINCH,
            GestureIntent.PAN,
            GestureIntent.SWIPE,
            GestureIntent.LONG_PRESS,
            GestureIntent.DOUBLE_TAP,
            GestureIntent.SINGLE_TAP,
        ).inOrder()
    }

    @Test
    fun newPointerSequenceClearsPreviousDecision() {
        val arbiter = GestureArbiter()
        arbiter.accept(GestureIntent.PINCH)
        arbiter.beginPointerSequence()

        assertThat(arbiter.current()).isNull()
        assertThat(arbiter.accept(GestureIntent.SINGLE_TAP)).isTrue()
    }

    @Test
    fun lateRecognizerBeginDoesNotResetTransformWinner() {
        val arbiter = GestureArbiter()
        arbiter.beginPointerSequence()
        arbiter.accept(GestureIntent.PAN)

        // A tap recognizer may observe the same down after the transform recognizer.
        arbiter.beginPointerSequence()

        assertThat(arbiter.current()).isEqualTo(GestureIntent.PAN)
        assertThat(arbiter.accept(GestureIntent.SINGLE_TAP)).isFalse()
        arbiter.endPointerSequence()
        arbiter.beginPointerSequence()
        assertThat(arbiter.current()).isNull()
    }
}
