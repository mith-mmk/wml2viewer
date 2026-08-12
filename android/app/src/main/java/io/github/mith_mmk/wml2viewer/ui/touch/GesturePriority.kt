package io.github.mith_mmk.wml2viewer.ui.touch

enum class GestureIntent(val priority: Int) {
    SINGLE_TAP(1),
    DOUBLE_TAP(2),
    LONG_PRESS(3),
    SWIPE(4),
    PAN(5),
    PINCH(6),
}

/** Higher-priority gestures replace lower-priority candidates in one pointer sequence. */
class GestureArbiter {
    private var accepted: GestureIntent? = null
    private var pointerSequenceActive = false

    @Synchronized
    fun accept(intent: GestureIntent): Boolean {
        val current = accepted
        if (current != null && current.priority > intent.priority) return false
        accepted = intent
        return true
    }

    @Synchronized
    fun current(): GestureIntent? = accepted

    /**
     * Called by every recognizer at pointer-down. Only the first observer resets state, so a
     * transform that wins before the tap observer runs cannot be accidentally downgraded.
     */
    @Synchronized
    fun beginPointerSequence() {
        if (pointerSequenceActive) return
        accepted = null
        pointerSequenceActive = true
    }

    @Synchronized
    fun endPointerSequence() {
        pointerSequenceActive = false
    }

    @Synchronized
    fun reset() {
        accepted = null
        pointerSequenceActive = false
    }
}
