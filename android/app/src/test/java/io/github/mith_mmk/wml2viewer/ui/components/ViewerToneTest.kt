package io.github.mith_mmk.wml2viewer.ui.components

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class ViewerToneTest {
    @Test
    fun grayscaleMapsToZeroSaturationAndColorMapsToFullSaturation() {
        assertThat(viewerSaturation(grayscaleEnabled = true)).isEqualTo(0f)
        assertThat(viewerSaturation(grayscaleEnabled = false)).isEqualTo(1f)
    }
}
