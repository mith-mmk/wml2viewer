package io.github.mith_mmk.wml2viewer.platform.smb

import java.io.IOException
import java.util.Collections
import java.util.IdentityHashMap
import java.util.concurrent.TimeoutException

internal fun Throwable.isRetryableSmbNetworkFailure(): Boolean {
    val visited = Collections.newSetFromMap(IdentityHashMap<Throwable, Boolean>())
    var current: Throwable? = this
    while (current != null && visited.add(current)) {
        if (current is IOException || current is TimeoutException) return true
        current = current.cause
    }
    return false
}
