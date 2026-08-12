package io.github.mith_mmk.wml2viewer.data.config

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.config.proto.CodecPolicyV1
import io.github.mith_mmk.wml2viewer.data.config.proto.MobileConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ThemeModeV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ViewerActionV1
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import kotlinx.coroutines.test.runTest
import org.junit.Test

class MobileConfigSerializerTest {
    @Test
    fun defaultsAreMobileOnlyAndMatchTouchPreset() {
        val config = MobileConfigSerializer.defaultValue

        assertThat(config.schemaVersion).isEqualTo(1)
        assertThat(config.localeAppearance.theme).isEqualTo(ThemeModeV1.THEME_MODE_CINEMATIC_DARK)
        assertThat(config.localeAppearance.dynamicColor).isFalse()
        assertThat(config.touch.swipeEnabled).isFalse()
        assertThat(config.touch.pinchZoomEnabled).isTrue()
        assertThat(config.touch.bindingsList.map { it.action }).containsExactly(
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_FILER,
            ViewerActionV1.VIEWER_ACTION_NEXT,
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_SETTINGS,
            ViewerActionV1.VIEWER_ACTION_NEXT,
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_SUB_FILER,
            ViewerActionV1.VIEWER_ACTION_NEXT,
        ).inOrder()
        assertThat(config.codec.defaultPolicy).isEqualTo(CodecPolicyV1.CODEC_POLICY_INTERNAL_FIRST)
    }

    @Test
    fun protobufRoundTripKeepsVersionedConfig() = runTest {
        val output = ByteArrayOutputStream()
        MobileConfigSerializer.writeTo(MobileConfigSerializer.defaultValue, output)

        val decoded = MobileConfigSerializer.readFrom(ByteArrayInputStream(output.toByteArray()))

        assertThat(decoded).isEqualTo(MobileConfigSerializer.defaultValue)
        assertThat(MobileConfigV1.parseFrom(output.toByteArray()).schemaVersion).isEqualTo(1)
    }
}
