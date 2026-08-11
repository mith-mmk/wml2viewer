package io.github.mith_mmk.wml2viewer.ui.state

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.SettingsCategory
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import org.junit.Test

class MobileViewerDefaultsTest {
    @Test
    fun mobileOnlyCategoriesAndDefaultsAreStable() {
        val settings = MobileViewerSettings()

        assertThat(SettingsCategory.entries).hasSize(8)
        assertThat(settings.codecs.defaultPolicy).isEqualTo(CodecPolicy.INTERNAL_FIRST)
        assertThat(settings.theme).isEqualTo(ThemeMode.CINEMATIC_DARK)
        assertThat(settings.dynamicColor).isFalse()
        assertThat(settings.manga.divider).isFalse()
        assertThat(settings.manga.prefetchSpreads).isEqualTo(1)
        assertThat(settings.gestures.swipeEnabled).isFalse()
        assertThat(settings.gestures.pinchZoom).isTrue()
    }

    @Test
    fun smbDefaultsToBestEffortAndPasswordCanBeCleared() {
        val input = SmbConnectionInput(server = "nas", password = charArrayOf('s', 'e', 'c'))

        assertThat(input.requireEncryption).isFalse()
        input.clearPassword()
        assertThat(input.password).asList().containsExactly('\u0000', '\u0000', '\u0000')
    }

    @Test
    fun nativeErrorCodesMapToLocalizedDomainIdentity() {
        assertThat(NativeUiErrorMapper.fromCode(1)).isEqualTo(UiErrorCode.INVALID_HANDLE)
        assertThat(NativeUiErrorMapper.fromCode(7)).isEqualTo(UiErrorCode.LIMIT)
        assertThat(NativeUiErrorMapper.fromCode(8)).isEqualTo(UiErrorCode.ENCODE)
        assertThat(NativeUiErrorMapper.fromCode(999)).isEqualTo(UiErrorCode.UNKNOWN)
    }
}
