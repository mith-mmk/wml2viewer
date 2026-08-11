package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Test

class SourceRootOperationPolicyTest {
    @Test
    fun pseudoRootRowMasksEveryMutationCapability() {
        val capabilities = sourceRootRowCapabilities()

        assertThat(capabilities.canList).isTrue()
        assertThat(capabilities.canCreate).isFalse()
        assertThat(capabilities.canCopyWithinProvider).isFalse()
        assertThat(capabilities.canMoveWithinProvider).isFalse()
        assertThat(capabilities.canRename).isFalse()
        assertThat(capabilities.canTrash).isFalse()
        assertThat(capabilities.canDelete).isFalse()
        assertThat(capabilities.canCopyDirectoriesWithinProvider).isFalse()
        assertThat(capabilities.canMoveDirectoriesWithinProvider).isFalse()
        assertThat(capabilities.canTransferDirectoriesAcrossProviders).isFalse()
    }

    @Test
    fun sourceRootGuardRejectsBeforeProviderOperation() {
        val root = EntryRef("saf:test", "root")
        var providerReached = false

        val error = assertThrows(SourceException::class.java) {
            runTest {
                withNonRootSourceOperation(root, root) {
                    providerReached = true
                }
            }
        }

        assertThat(error.code).isEqualTo(SourceErrorCode.ACCESS_DENIED)
        assertThat(providerReached).isFalse()
    }

    @Test
    fun ordinaryEntryStillReachesProviderOperation() = runTest {
        val root = EntryRef("saf:test", "root")
        val child = EntryRef("saf:test", "child")
        var providerReached = false

        withNonRootSourceOperation(child, root) { providerReached = true }

        assertThat(providerReached).isTrue()
    }
}
