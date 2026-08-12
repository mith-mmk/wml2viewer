package io.github.mith_mmk.wml2viewer

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class LegacyAndroidResetTest {
    @Test
    fun resetTargetsOnlyLegacyAppPrivateState() {
        assertThat(LegacyAndroidReset.pathsForTest()).containsExactly(
            "imported",
            ".importing",
            "config",
            "picker.request",
            "import.ready",
        )
        assertThat(LegacyAndroidReset.pathsForTest().any { it.contains("..") }).isFalse()
    }

    @Test
    fun retryOnlyTargetsGrantsCapturedBeforeV2UiStarts() {
        val captured = pendingLegacyGrantUris(
            captured = false,
            saved = emptySet(),
            current = setOf("content://legacy/tree"),
        )
        val retry = pendingLegacyGrantUris(
            captured = true,
            saved = captured,
            current = captured + "content://v2/tree",
        )

        assertThat(retry).containsExactly("content://legacy/tree")
    }
}
