package io.github.mith_mmk.wml2viewer.ui.components

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import org.junit.Test

class ExportDialogLogicTest {
    @Test
    fun defaultNameRemovesPathAndReplacesUnsafeCharacters() {
        assertThat(defaultExportFileName("folder/page:01.avif", ExportFormat.PNG))
            .isEqualTo("page_01.png")
        assertThat(defaultExportFileName("", ExportFormat.JPEG)).isEqualTo("export.jpg")
    }

    @Test
    fun normalizationReplacesKnownExportExtensionAndRejectsPaths() {
        assertThat(normalizeExportFileName("page.png", ExportFormat.WEBP_LOSSLESS))
            .isEqualTo("page.webp")
        assertThat(normalizeExportFileName("page.final", ExportFormat.JPEG))
            .isEqualTo("page.final.jpg")
        assertThat(normalizeExportFileName("../page", ExportFormat.PNG)).isNull()
        assertThat(normalizeExportFileName("..", ExportFormat.PNG)).isNull()
    }
}
