package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceCapabilities
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import kotlinx.coroutines.test.runTest
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class OsAnimationCompatibilityTest {
    private val page = ViewerPageSource.Direct(
        SourceEntry(
            ref = EntryRef("test", "page"),
            parent = null,
            name = "page.gif",
            kind = EntryKind.FILE,
            mimeType = "image/gif",
            size = null,
            modifiedAtEpochMillis = null,
            isHidden = false,
            effectiveCapabilities = SourceCapabilities(),
        ),
    )

    @Test
    fun animatedOsPosterPrefersInternalFrames() = runTest {
        val poster = loaded("os", animatedPoster = true)
        val animated = loaded("internal", animatedPoster = false)

        val result = preferInternalForAnimatedOsPoster(
            os = { poster },
            internal = { animated },
        )

        assertThat(result).isSameInstanceAs(animated)
    }

    @Test
    fun animatedOsPosterFailsExplicitlyWhenInternalDecodeFails() = runTest {
        val poster = loaded("os", animatedPoster = true)

        val failure = try {
            preferInternalForAnimatedOsPoster(
                os = { poster },
                internal = { error("decode failed") },
            )
            null
        } catch (error: Throwable) {
            error
        }

        assertThat(failure).isInstanceOf(OsAnimatedPlaybackUnsupportedException::class.java)
    }

    @Test
    fun staticOsResultDoesNotInvokeInternalDecoder() = runTest {
        val poster = loaded("os", animatedPoster = false)
        var internalCalled = false

        val result = preferInternalForAnimatedOsPoster(
            os = { poster },
            internal = {
                internalCalled = true
                loaded("internal", animatedPoster = false)
            },
        )

        assertThat(result).isSameInstanceAs(poster)
        assertThat(internalCalled).isFalse()
    }

    @Test
    fun internalFailureRejectsAnimatedOsFallback() = runTest {
        val failure = try {
            preferInternalWithStaticOsFallback(
                internal = { error("decode failed") },
                os = { loaded("os", animatedPoster = true) },
            )
            null
        } catch (error: Throwable) {
            error
        }

        assertThat(failure).isInstanceOf(OsAnimatedPlaybackUnsupportedException::class.java)
    }

    @Test
    fun internalFailureAcceptsStaticOsFallback() = runTest {
        val poster = loaded("os", animatedPoster = false)

        val result = preferInternalWithStaticOsFallback(
            internal = { error("decode failed") },
            os = { poster },
        )

        assertThat(result).isSameInstanceAs(poster)
    }

    private fun loaded(id: String, animatedPoster: Boolean) = LoadedViewerPage(
        page = page.copy(id = id),
        frame = TestImageBitmap(1, 1),
        portrait = true,
        osAnimatedPoster = animatedPoster,
    )
}
