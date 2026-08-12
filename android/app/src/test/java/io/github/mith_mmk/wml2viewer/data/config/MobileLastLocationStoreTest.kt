package io.github.mith_mmk.wml2viewer.data.config

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.config.proto.MobileConfigV1
import org.junit.Test

class MobileLastLocationStoreTest {
    @Test
    fun locationRoundTripUsesOnlyOpaqueNonSecretIdentity() {
        val location = MobileLastLocation(
            sourceId = "smb:profile-id",
            directoryOpaqueEntryId = "opaque-directory",
            openedEntryOpaqueEntryId = "opaque-archive",
            logicalPageIndex = 27,
            openedArchive = true,
        )

        val encoded = MobileConfigV1.getDefaultInstance().withLastLocation(location)

        assertThat(encoded.toLastLocation()).isEqualTo(location)
        assertThat(encoded.toByteArray().toString(Charsets.ISO_8859_1)).doesNotContain("password")
    }

    @Test
    fun clearingLocationPreservesUnrelatedConfiguration() {
        val initial = MobileConfigSerializer.defaultValue
            .withLastLocation(
                MobileLastLocation("saf:one", "directory", "page", 2, openedArchive = false),
            )

        val cleared = initial.withLastLocation(null)

        assertThat(cleared.hasLastLocation()).isFalse()
        assertThat(cleared.filer).isEqualTo(initial.filer)
        assertThat(cleared.schemaVersion).isEqualTo(initial.schemaVersion)
    }

    @Test
    fun durableRememberFlagAndLocationAreReadFromTheSameSnapshot() {
        val location = MobileLastLocation("saf:one", "directory", "page", 2, openedArchive = true)
        val disabled = MobileConfigSerializer.defaultValue.toBuilder()
            .setFiler(MobileConfigSerializer.defaultValue.filer.toBuilder().setRememberLastLocation(false))
            .build()
            .withLastLocation(location)
        val enabled = disabled.toBuilder()
            .setFiler(disabled.filer.toBuilder().setRememberLastLocation(true))
            .build()

        assertThat(disabled.toRememberedLastLocation()).isNull()
        assertThat(enabled.toRememberedLastLocation()).isEqualTo(location)
    }
}
