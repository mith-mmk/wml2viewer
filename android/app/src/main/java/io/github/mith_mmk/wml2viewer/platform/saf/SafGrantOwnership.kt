package io.github.mith_mmk.wml2viewer.platform.saf

internal data class PersistedSafGrantDelta(val read: Boolean, val write: Boolean)

internal fun newlyPersistedSafGrantModes(
    requestedRead: Boolean,
    requestedWrite: Boolean,
    existingRead: Boolean,
    existingWrite: Boolean,
): PersistedSafGrantDelta = PersistedSafGrantDelta(
    read = requestedRead && !existingRead,
    write = requestedWrite && !existingWrite,
)

internal fun orphanedPersistedSafGrantUris(
    profileUris: Set<String>,
    persistedUris: Set<String>,
): Set<String> = persistedUris - profileUris
