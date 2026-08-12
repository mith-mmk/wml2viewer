package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import org.junit.Assert.assertThrows
import org.junit.Test

class MobileViewerSupportTest {
    @Test
    fun exportNameDropsPathAndReplacesExtension() {
        assertThat(normalizedExportName("folder\\page.old", ExportFormat.PNG)).isEqualTo("page.png")
        assertThat(normalizedExportName("page", ExportFormat.JPEG)).isEqualTo("page.jpg")
    }

    @Test
    fun exportCollisionIsExactAndEveryResolutionMapsToProviderPolicy() {
        assertThat(exportNameCollides("page.png", listOf("page.jpg", "page.png"))).isTrue()
        assertThat(exportNameCollides("PAGE.PNG", listOf("page.png"))).isFalse()
        assertThat(CollisionResolution.REPLACE.toCollisionPolicy()).isEqualTo(CollisionPolicy.REPLACE)
        assertThat(CollisionResolution.KEEP_BOTH.toCollisionPolicy()).isEqualTo(CollisionPolicy.KEEP_BOTH)
        assertThat(CollisionResolution.SKIP.toCollisionPolicy()).isEqualTo(CollisionPolicy.SKIP)
    }

    @Test
    fun entryTokenRoundTripsOpaqueProviderIdentity() {
        val ref = EntryRef("saf:provider", "content-like/opaque?id=12")
        assertThat(EntryUiTokenCodec.decode(EntryUiTokenCodec.encode(ref))).isEqualTo(ref)
        assertThrows(IllegalArgumentException::class.java) { EntryUiTokenCodec.decode("not-an-entry") }
    }

    @Test
    fun naturalOrderKeepsComicPagesNumericallyOrdered() {
        val names = listOf("page10.png", "page02.png", "page1.png", "page2.png")
        assertThat(names.sortedWith(NaturalFileNameComparator))
            .containsExactly("page1.png", "page2.png", "page02.png", "page10.png")
            .inOrder()
    }

    @Test
    fun automaticCacheLimitUsesTenPercentBoundsAndReserve() {
        assertThat(MobileCacheLimitPolicy.automatic(20L * MobileCacheLimitPolicy.GIB).maxBytes)
            .isEqualTo(2L * MobileCacheLimitPolicy.GIB)
        assertThat(MobileCacheLimitPolicy.automatic(5L * MobileCacheLimitPolicy.GIB).maxBytes)
            .isEqualTo(512L * MobileCacheLimitPolicy.MIB)
        val low = MobileCacheLimitPolicy.automatic(1100L * MobileCacheLimitPolicy.MIB)
        assertThat(low.lowSpace).isTrue()
        assertThat(low.maxBytes).isEqualTo(76L * MobileCacheLimitPolicy.MIB)
    }

    @Test
    fun fileTypesRecognizeImagesAndSupportedVirtualContainers() {
        assertThat(MobileFileTypes.isImage("PAGE.AVIF", null)).isTrue()
        assertThat(MobileFileTypes.mimeType("page.avif", "application/octet-stream"))
            .isEqualTo("image/avif")
        assertThat(MobileFileTypes.mimeType("page.webp", "image/unknown"))
            .isEqualTo("image/webp")
        assertThat(MobileFileTypes.mimeType("page.bin", "IMAGE/PNG; charset=binary"))
            .isEqualTo("image/png")
        assertThat(MobileFileTypes.archiveFormat("book.cbz")).isEqualTo("zip")
        assertThat(MobileFileTypes.archiveFormat("list.wmltxt")).isEqualTo("wmltxt")
        listOf("jpe", "mag", "mki", "pcd", "pi", "pic", "vsp").forEach { extension ->
            assertThat(MobileFileTypes.isImage("legacy.$extension", "application/octet-stream")).isTrue()
        }
        listOf("jxl", "pnm", "ppm", "qoi", "svg", "tga").forEach { extension ->
            assertThat(MobileFileTypes.isImage("unsupported.$extension", "application/octet-stream")).isFalse()
        }
        assertThat(MobileFileTypes.isImage("notes.txt", "text/plain")).isFalse()
    }
}
