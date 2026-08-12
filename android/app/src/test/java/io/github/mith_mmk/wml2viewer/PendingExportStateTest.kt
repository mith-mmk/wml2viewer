package io.github.mith_mmk.wml2viewer

import android.os.Bundle
import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class PendingExportStateTest {
    @Test
    fun requestRoundTripsAcrossActivityRecreation() {
        val request = ExportRequest(
            format = ExportFormat.WEBP_LOSSLESS,
            quality = 100,
            fileName = "page.webp",
            destination = ExportDestination.SYSTEM_PICKER,
        )
        val state = Bundle()

        PendingExportState.save(state, request)

        assertThat(PendingExportState.restore(state)).isEqualTo(request)
    }

    @Test
    fun invalidStateIsRejected() {
        val state = Bundle().apply {
            putString("pending_export_format", "NOT_A_FORMAT")
            putString("pending_export_destination", ExportDestination.SYSTEM_PICKER.name)
            putInt("pending_export_quality", 90)
            putString("pending_export_file_name", "page.png")
        }

        assertThat(PendingExportState.restore(state)).isNull()
    }
}
