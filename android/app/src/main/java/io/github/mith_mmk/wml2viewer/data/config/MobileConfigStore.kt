package io.github.mith_mmk.wml2viewer.data.config

import android.content.Context
import androidx.datastore.core.CorruptionException
import androidx.datastore.core.DataStore
import androidx.datastore.core.Serializer
import androidx.datastore.dataStore
import com.google.protobuf.InvalidProtocolBufferException
import io.github.mith_mmk.wml2viewer.data.config.proto.CacheConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.CodecConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.CodecPolicyV1
import io.github.mith_mmk.wml2viewer.data.config.proto.DisplayConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.DisplayFitV1
import io.github.mith_mmk.wml2viewer.data.config.proto.FilerConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.LocaleAppearanceConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.MangaConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.MangaLayoutV1
import io.github.mith_mmk.wml2viewer.data.config.proto.MobileConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ReadingDirectionV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SortOrderV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ThemeModeV1
import io.github.mith_mmk.wml2viewer.data.config.proto.TextScaleV1
import io.github.mith_mmk.wml2viewer.data.config.proto.TouchBindingV1
import io.github.mith_mmk.wml2viewer.data.config.proto.TouchConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ViewerActionV1
import java.io.InputStream
import java.io.OutputStream
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first

object MobileConfigSerializer : Serializer<MobileConfigV1> {
    override val defaultValue: MobileConfigV1 = MobileConfigV1.newBuilder()
        .setSchemaVersion(1)
        .setDisplay(
            DisplayConfigV1.newBuilder()
                .setEdgeToEdge(true)
                .setFit(DisplayFitV1.DISPLAY_FIT_CONTAIN)
                .setShowTopChrome(true)
                .setShowFilmstrip(true),
        )
        .setManga(
            MangaConfigV1.newBuilder()
                .setLayout(MangaLayoutV1.MANGA_LAYOUT_AUTO)
                .setReadingDirection(ReadingDirectionV1.READING_DIRECTION_RTL)
                .setCoverAlone(true)
                .setLandscapeSpread(true)
                .setPrefetchSpreads(1),
        )
        .setTouch(defaultTouchConfig())
        .setFiler(
            FilerConfigV1.newBuilder()
                .setSortOrder(SortOrderV1.SORT_ORDER_NAME_ASCENDING)
                .setRememberLastLocation(true),
        )
        .setCodec(
            CodecConfigV1.newBuilder()
                .setDefaultPolicy(CodecPolicyV1.CODEC_POLICY_INTERNAL_FIRST),
        )
        .setLocaleAppearance(
            LocaleAppearanceConfigV1.newBuilder()
                .setTheme(ThemeModeV1.THEME_MODE_CINEMATIC_DARK)
                .setDynamicColor(false)
                .setTextScale(TextScaleV1.TEXT_SCALE_MEDIUM),
        )
        .setCache(CacheConfigV1.newBuilder().setAutomaticLimit(true))
        .build()

    override suspend fun readFrom(input: InputStream): MobileConfigV1 = try {
        MobileConfigV1.parseFrom(input).takeIf { it.schemaVersion == 1 } ?: defaultValue
    } catch (error: InvalidProtocolBufferException) {
        throw CorruptionException("Cannot read MobileConfigV1", error)
    }

    override suspend fun writeTo(t: MobileConfigV1, output: OutputStream) {
        t.writeTo(output)
    }

    private fun defaultTouchConfig(): TouchConfigV1 {
        val actions = listOf(
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_FILER,
            ViewerActionV1.VIEWER_ACTION_NEXT,
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_SETTINGS,
            ViewerActionV1.VIEWER_ACTION_NEXT,
            ViewerActionV1.VIEWER_ACTION_PREVIOUS,
            ViewerActionV1.VIEWER_ACTION_OPEN_SUB_FILER,
            ViewerActionV1.VIEWER_ACTION_NEXT,
        )
        return TouchConfigV1.newBuilder()
            .addAllBindings(actions.mapIndexed { index, action ->
                TouchBindingV1.newBuilder().setCell(index).setAction(action).build()
            })
            .setSwipeEnabled(false)
            .setPinchZoomEnabled(true)
            .setPanEnabled(true)
            .setDoubleTapAction(ViewerActionV1.VIEWER_ACTION_TOGGLE_FIT)
            .setLongPressAction(ViewerActionV1.VIEWER_ACTION_QUICK_MENU)
            .build()
    }
}

private val Context.mobileConfigDataStore: DataStore<MobileConfigV1> by dataStore(
    fileName = "mobile_config_v1.pb",
    serializer = MobileConfigSerializer,
)

class MobileConfigRepository(context: Context) {
    private val store = context.applicationContext.mobileConfigDataStore

    val config: Flow<MobileConfigV1> = store.data

    suspend fun current(): MobileConfigV1 = config.first()

    suspend fun update(transform: (MobileConfigV1) -> MobileConfigV1): MobileConfigV1 =
        store.updateData(transform)
}
