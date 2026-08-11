package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput

/** Consumes otherwise-unhandled panel input after child controls have had their turn. */
fun Modifier.consumeViewerInput(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Final)
            event.changes.forEach { change ->
                if (!change.isConsumed) change.consume()
            }
        }
    }
}
