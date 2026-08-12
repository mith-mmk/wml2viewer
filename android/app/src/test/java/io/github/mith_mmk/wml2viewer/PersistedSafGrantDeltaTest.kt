package io.github.mith_mmk.wml2viewer

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.platform.saf.PersistedSafGrantDelta
import io.github.mith_mmk.wml2viewer.platform.saf.newlyPersistedSafGrantModes
import io.github.mith_mmk.wml2viewer.platform.saf.orphanedPersistedSafGrantUris
import org.junit.Test

class PersistedSafGrantDeltaTest {
    @Test
    fun newGrantOwnsEveryRequestedMode() {
        assertThat(
            newlyPersistedSafGrantModes(
                requestedRead = true,
                requestedWrite = true,
                existingRead = false,
                existingWrite = false,
            ),
        ).isEqualTo(PersistedSafGrantDelta(read = true, write = true))
    }

    @Test
    fun existingModesAreNeverReleasedByRegistrationRollback() {
        assertThat(
            newlyPersistedSafGrantModes(
                requestedRead = true,
                requestedWrite = true,
                existingRead = true,
                existingWrite = false,
            ),
        ).isEqualTo(PersistedSafGrantDelta(read = false, write = true))
    }

    @Test
    fun duplicateGrantOwnsNoModes() {
        assertThat(
            newlyPersistedSafGrantModes(
                requestedRead = true,
                requestedWrite = true,
                existingRead = true,
                existingWrite = true,
            ),
        ).isEqualTo(PersistedSafGrantDelta(read = false, write = false))
    }

    @Test
    fun startupReconciliationKeepsProfileOwnedGrantsAndFindsOnlyOrphans() {
        assertThat(
            orphanedPersistedSafGrantUris(
                profileUris = setOf("content://documents/tree/owned"),
                persistedUris = setOf(
                    "content://documents/tree/owned",
                    "content://documents/tree/interrupted-picker",
                ),
            ),
        ).containsExactly("content://documents/tree/interrupted-picker")
    }
}
