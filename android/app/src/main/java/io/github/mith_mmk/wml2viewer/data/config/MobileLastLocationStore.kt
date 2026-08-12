package io.github.mith_mmk.wml2viewer.data.config

import io.github.mith_mmk.wml2viewer.data.config.proto.LastLocationV1
import io.github.mith_mmk.wml2viewer.data.config.proto.MobileConfigV1

data class MobileLastLocation(
    val sourceId: String,
    val directoryOpaqueEntryId: String,
    val openedEntryOpaqueEntryId: String? = null,
    val logicalPageIndex: Int = 0,
    val openedArchive: Boolean = false,
)

class MobileLastLocationStore(
    private val repository: MobileConfigRepository,
) {
    suspend fun current(): MobileLastLocation? = repository.current().toLastLocation()

    /** Reads the enable flag and location from one durable DataStore snapshot. */
    suspend fun currentIfRemembered(): MobileLastLocation? = repository.current().toRememberedLastLocation()

    suspend fun replace(location: MobileLastLocation?) {
        repository.update { config -> config.withLastLocation(location) }
    }
}

internal fun MobileConfigV1.toRememberedLastLocation(): MobileLastLocation? =
    if (filer.rememberLastLocation) toLastLocation() else null

internal fun MobileConfigV1.toLastLocation(): MobileLastLocation? {
    if (!hasLastLocation()) return null
    val location = lastLocation
    if (location.sourceId.isBlank() || location.directoryOpaqueEntryId.isBlank()) return null
    return MobileLastLocation(
        sourceId = location.sourceId,
        directoryOpaqueEntryId = location.directoryOpaqueEntryId,
        openedEntryOpaqueEntryId = location.openedEntryOpaqueEntryId.takeIf(String::isNotBlank),
        logicalPageIndex = location.logicalPageIndex.toInt().coerceAtLeast(0),
        openedArchive = location.openedArchive,
    )
}

internal fun MobileConfigV1.withLastLocation(location: MobileLastLocation?): MobileConfigV1 {
    val builder = toBuilder()
    if (location == null) return builder.clearLastLocation().build()
    require(location.sourceId.isNotBlank()) { "Last-location source ID must not be blank" }
    require(location.directoryOpaqueEntryId.isNotBlank()) { "Last-location directory ID must not be blank" }
    require(location.logicalPageIndex >= 0) { "Last-location page index must not be negative" }
    return builder.setLastLocation(
        LastLocationV1.newBuilder()
            .setSourceId(location.sourceId)
            .setDirectoryOpaqueEntryId(location.directoryOpaqueEntryId)
            .setOpenedEntryOpaqueEntryId(location.openedEntryOpaqueEntryId.orEmpty())
            .setLogicalPageIndex(location.logicalPageIndex)
            .setOpenedArchive(location.openedArchive),
    ).build()
}
