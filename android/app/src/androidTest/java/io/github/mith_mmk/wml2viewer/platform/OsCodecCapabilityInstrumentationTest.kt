package io.github.mith_mmk.wml2viewer.platform.codec

import android.graphics.Bitmap
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class OsCodecCapabilityInstrumentationTest {
    @Test
    fun everyAdvertisedOsEncoderCompletesAnImageDecoderRoundTrip() = runBlocking {
        val probe = AndroidCodecCapabilityProbe()
        val capabilities = probe.probe()
        assertTrue("PNG is required by the Android platform", OsEncodeFormat.PNG in capabilities.encodableFormats)
        assertFalse(capabilities.encodableFormats.isEmpty())

        val osOnly = CodecRoutePolicy(global = CodecRoute.OS_ONLY)
        val router = AndroidCodecRouter(routePolicy = osOnly)
        val source = Bitmap.createBitmap(3, 2, Bitmap.Config.ARGB_8888).apply {
            setPixels(
                intArrayOf(
                    0xFFFF0000.toInt(),
                    0xFF00FF00.toInt(),
                    0xFF0000FF.toInt(),
                    0xFFFFFFFF.toInt(),
                    0xFF000000.toInt(),
                    0xFF7F7F7F.toInt(),
                ),
                0,
                3,
                0,
                0,
                3,
                2,
            )
        }
        try {
            capabilities.encodableFormats.forEach { format ->
                val encoded = router.encode(source, format, quality = 91)
                assertFalse("$format produced no bytes", encoded.isEmpty())
                router.decodeWithPolicy(encoded, format.mimeType, policy = osOnly).use { decoded ->
                    assertEquals("$format width", source.width, decoded.sourceWidth)
                    assertEquals("$format height", source.height, decoded.sourceHeight)
                    assertEquals("$format decoded width", source.width, decoded.bitmap.width)
                    assertEquals("$format decoded height", source.height, decoded.bitmap.height)
                }
            }
        } finally {
            source.recycle()
        }
    }

    @Test
    fun capabilityProbeOnlyClaimsFormatsThatPassedItsRoundTrip() {
        val probe = AndroidCodecCapabilityProbe()
        val capabilities = probe.probe()

        capabilities.decodableMimeTypes.forEach { mimeType ->
            assertTrue(probe.canDecode(mimeType.uppercase()))
        }
        assertFalse(probe.canDecode("image/not-a-codec"))
        assertFalse(probe.canDecode(null))
    }

    @Test
    fun fixtureDecodersMatchMeasuredFormatsAndRequiredAvifCapability() = runBlocking {
        val capabilities = AndroidCodecCapabilityProbe().probe()
        val router = AndroidCodecRouter(routePolicy = CodecRoutePolicy(global = CodecRoute.OS_ONLY))

        setOf(CodecFormat.GIF, CodecFormat.BMP, CodecFormat.ICO).forEach { format ->
            assertTrue("Android ImageDecoder must decode the $format probe", format in capabilities.decodableFormats)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            assertTrue("Android 14+ must pass the AVIF device probe", CodecFormat.AVIF in capabilities.decodableFormats)
        }

        AndroidCodecProbeFixtures.encoded.forEach { (format, bytes) ->
            if (format in capabilities.decodableFormats) {
                router.decodeWithPolicy(
                    encoded = bytes,
                    mimeType = format.probeMimeType,
                    policy = CodecRoutePolicy(global = CodecRoute.OS_ONLY),
                ).use { decoded ->
                    assertTrue("$format width", decoded.sourceWidth > 0)
                    assertTrue("$format height", decoded.sourceHeight > 0)
                }
            }
        }
    }

    private val CodecFormat.probeMimeType: String
        get() = when (this) {
            CodecFormat.GIF -> "image/gif"
            CodecFormat.BMP -> "image/bmp"
            CodecFormat.ICO -> "image/x-icon"
            CodecFormat.HEIF -> "image/heif"
            CodecFormat.AVIF -> "image/avif"
            CodecFormat.DNG -> "image/dng"
            else -> error("No fixture MIME for $this")
        }
}
