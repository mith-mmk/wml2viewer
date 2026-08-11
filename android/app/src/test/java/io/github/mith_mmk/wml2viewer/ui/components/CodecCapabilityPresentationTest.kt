package io.github.mith_mmk.wml2viewer.ui.components

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import org.junit.Test

class CodecCapabilityPresentationTest {
    @Test
    fun measuredFormatsAreShownInStableUiOrderOnly() {
        val result = orderedMeasuredCodecFormats(
            setOf(CodecFormat.AVIF, CodecFormat.JPEG, CodecFormat.WEBP),
        )

        assertThat(result).containsExactly(
            CodecFormat.JPEG,
            CodecFormat.WEBP,
            CodecFormat.AVIF,
        ).inOrder()
    }

    @Test
    fun emptyProbeResultDoesNotInventCapabilities() {
        assertThat(orderedMeasuredCodecFormats(emptySet())).isEmpty()
    }

    @Test
    fun unmeasuredFormatCannotSelectAnOsRoute() {
        assertThat(selectableCodecPolicies(osSupported = false)).containsExactly(
            CodecPolicy.DEFAULT,
            CodecPolicy.INTERNAL_FIRST,
            CodecPolicy.INTERNAL_ONLY,
        ).inOrder()
    }
}
