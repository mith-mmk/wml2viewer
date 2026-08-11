package io.github.mith_mmk.wml2viewer.data.config

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import org.junit.Test

class ProtoMobileSettingsStoreTest {
    @Test
    fun domainRoundTripPreservesEveryMobileSettingAndSourceProfiles() {
        val original = MobileConfigSerializer.defaultValue.toBuilder()
            .addSources(
                io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1.newBuilder()
                    .setSourceId("source-1")
                    .setDisplayName("NAS")
                    .setSmb(
                        io.github.mith_mmk.wml2viewer.data.config.proto.SmbProfileV1.newBuilder()
                            .setServer("nas.invalid")
                            .setCredentialId("credential-1"),
                    ),
            )
            .build()
        val settings = original.toUiSettings().copy(
            theme = ThemeMode.LIGHT,
            codecs = original.toUiSettings().codecs
                .withOverride(CodecFormat.AVIF, CodecPolicy.OS_ONLY),
            touchMap = original.toUiSettings().touchMap
                .withBinding(TapZone.CENTER, ViewerAction.EXPORT),
        )

        val roundTripped = original.withUiSettings(settings)

        assertThat(roundTripped.toUiSettings()).isEqualTo(settings)
        assertThat(roundTripped.sourcesList).isEqualTo(original.sourcesList)
        assertThat(roundTripped.sourcesList.single().smb.credentialId).isEqualTo("credential-1")
    }
}
