package io.github.mith_mmk.wml2viewer.data.config

import io.github.mith_mmk.wml2viewer.data.config.proto.CodecOverrideV1
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
import io.github.mith_mmk.wml2viewer.data.config.proto.TextScaleV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ThemeModeV1
import io.github.mith_mmk.wml2viewer.data.config.proto.TouchBindingV1
import io.github.mith_mmk.wml2viewer.data.config.proto.TouchConfigV1
import io.github.mith_mmk.wml2viewer.data.config.proto.ViewerActionV1
import io.github.mith_mmk.wml2viewer.ui.model.CacheSettings
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.CodecSettings
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.FilerSettings
import io.github.mith_mmk.wml2viewer.ui.model.FilerSortOrder
import io.github.mith_mmk.wml2viewer.ui.model.GestureSettings
import io.github.mith_mmk.wml2viewer.ui.model.LanguagePreference
import io.github.mith_mmk.wml2viewer.ui.model.MangaLayoutMode
import io.github.mith_mmk.wml2viewer.ui.model.MangaSettings
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.ReadingDirection
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.TextScale
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import io.github.mith_mmk.wml2viewer.ui.model.TouchMapConfig
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.model.ViewingSettings
import io.github.mith_mmk.wml2viewer.ui.state.MobileSettingsStore
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.launch

class ProtoMobileSettingsStore(
    private val repository: MobileConfigRepository,
    scope: CoroutineScope,
) : MobileSettingsStore {
    private val mutableSettings = MutableStateFlow(MobileConfigSerializer.defaultValue.toUiSettings())
    override val settings: StateFlow<MobileViewerSettings> = mutableSettings

    init {
        scope.launch {
            repository.config
                .catch { error ->
                    if (error is IOException) emit(MobileConfigSerializer.defaultValue) else throw error
                }
                .collect { mutableSettings.value = it.toUiSettings() }
        }
    }

    override suspend fun replace(settings: MobileViewerSettings) {
        mutableSettings.value = settings
        repository.update { it.withUiSettings(settings) }
    }
}

internal fun MobileConfigV1.toUiSettings(): MobileViewerSettings {
    val touchBindings = TouchMapConfig.defaultBindings().toMutableMap()
    touch.bindingsList.forEach { binding ->
        TapZone.entries.getOrNull(binding.cell.toInt())?.let { zone ->
            touchBindings[zone] = binding.action.toUiAction()
        }
    }
    return MobileViewerSettings(
        viewing = ViewingSettings(
            edgeToEdge = display.edgeToEdge,
            keepScreenOn = display.keepScreenOn,
            fit = display.fit.toUiDisplayFit(),
            showTopChrome = display.showTopChrome,
            showFilmstrip = display.showFilmstrip,
        ),
        touchMap = TouchMapConfig(touchBindings),
        gestures = GestureSettings(
            swipeEnabled = touch.swipeEnabled,
            pinchZoom = touch.pinchZoomEnabled,
            pan = touch.panEnabled,
            doubleTapAction = touch.doubleTapAction.toUiAction(ViewerAction.TOGGLE_FIT_MODE),
            longPressAction = touch.longPressAction.toUiAction(ViewerAction.OPEN_CONTEXT_MENU),
        ),
        manga = MangaSettings(
            layoutMode = manga.layout.toUiMangaLayout(),
            readingDirection = manga.readingDirection.toUiReadingDirection(),
            singleCover = manga.coverAlone,
            divider = manga.divider,
            prefetchSpreads = manga.prefetchSpreads.toInt().coerceIn(0, 1),
        ),
        filer = FilerSettings(
            tabletFilmstripPinned = filer.tabletFilmstripPinned,
            showHiddenFiles = filer.showHidden,
            sortOrder = filer.sortOrder.toUiSortOrder(),
            rememberLastLocation = filer.rememberLastLocation,
        ),
        codecs = CodecSettings(
            defaultPolicy = codec.defaultPolicy.toUiCodecPolicy(CodecPolicy.INTERNAL_FIRST),
            overrides = codec.overridesList.mapNotNull { override ->
                runCatching { CodecFormat.valueOf(override.format.uppercase()) }.getOrNull()?.let { format ->
                    format to override.policy.toUiCodecPolicy()
                }
            }.filter { it.second != CodecPolicy.DEFAULT }.toMap(),
        ),
        language = LanguagePreference.entries.firstOrNull { it.tag == localeAppearance.languageTag }
            ?: LanguagePreference.SYSTEM,
        theme = localeAppearance.theme.toUiTheme(),
        dynamicColor = localeAppearance.dynamicColor,
        textScale = localeAppearance.textScale.toUiTextScale(),
        cache = CacheSettings(
            automaticLimit = cache.automaticLimit,
            manualLimitMiB = (cache.manualLimitBytes / MIB).toInt().coerceIn(256, 2_048),
        ),
    )
}

internal fun MobileConfigV1.withUiSettings(settings: MobileViewerSettings): MobileConfigV1 {
    val display = DisplayConfigV1.newBuilder()
        .setEdgeToEdge(settings.viewing.edgeToEdge)
        .setKeepScreenOn(settings.viewing.keepScreenOn)
        .setFit(settings.viewing.fit.toProto())
        .setShowTopChrome(settings.viewing.showTopChrome)
        .setShowFilmstrip(settings.viewing.showFilmstrip)
    val manga = MangaConfigV1.newBuilder()
        .setLayout(settings.manga.layoutMode.toProto())
        .setReadingDirection(settings.manga.readingDirection.toProto())
        .setCoverAlone(settings.manga.singleCover)
        .setDivider(settings.manga.divider)
        .setLandscapeSpread(settings.manga.layoutMode != MangaLayoutMode.SINGLE)
        .setPrefetchSpreads(settings.manga.prefetchSpreads.coerceIn(0, 1))
    val touch = TouchConfigV1.newBuilder()
        .addAllBindings(TapZone.entries.map { zone ->
            TouchBindingV1.newBuilder()
                .setCell(zone.ordinal)
                .setAction(settings.touchMap.actionFor(zone).toProto())
                .build()
        })
        .setSwipeEnabled(settings.gestures.swipeEnabled)
        .setPinchZoomEnabled(settings.gestures.pinchZoom)
        .setPanEnabled(settings.gestures.pan)
        .setDoubleTapAction(settings.gestures.doubleTapAction.toProto())
        .setLongPressAction(settings.gestures.longPressAction.toProto())
    val filer = FilerConfigV1.newBuilder(this.filer)
        .setTabletFilmstripPinned(settings.filer.tabletFilmstripPinned)
        .setShowHidden(settings.filer.showHiddenFiles)
        .setSortOrder(settings.filer.sortOrder.toProto())
        .setRememberLastLocation(settings.filer.rememberLastLocation)
    val codec = io.github.mith_mmk.wml2viewer.data.config.proto.CodecConfigV1.newBuilder()
        .setDefaultPolicy(settings.codecs.defaultPolicy.toProto())
        .addAllOverrides(settings.codecs.overrides.entries.sortedBy { it.key.name }.map { (format, policy) ->
            CodecOverrideV1.newBuilder().setFormat(format.name.lowercase()).setPolicy(policy.toProto()).build()
        })
    val locale = LocaleAppearanceConfigV1.newBuilder()
        .setLanguageTag(settings.language.tag.orEmpty())
        .setTheme(settings.theme.toProto())
        .setDynamicColor(settings.dynamicColor)
        .setTextScale(settings.textScale.toProto())
    return toBuilder()
        .setSchemaVersion(1)
        .setDisplay(display)
        .setManga(manga)
        .setTouch(touch)
        .setFiler(filer)
        .setCodec(codec)
        .setLocaleAppearance(locale)
        .setCache(
            io.github.mith_mmk.wml2viewer.data.config.proto.CacheConfigV1.newBuilder()
                .setAutomaticLimit(settings.cache.automaticLimit)
                .setManualLimitBytes(settings.cache.manualLimitMiB.coerceIn(256, 2_048) * MIB),
        )
        .build()
}

private fun ViewerActionV1.toUiAction(fallback: ViewerAction = ViewerAction.NONE): ViewerAction = when (this) {
    ViewerActionV1.VIEWER_ACTION_DISABLED -> ViewerAction.NONE
    ViewerActionV1.VIEWER_ACTION_PREVIOUS -> ViewerAction.PREVIOUS_IMAGE
    ViewerActionV1.VIEWER_ACTION_NEXT -> ViewerAction.NEXT_IMAGE
    ViewerActionV1.VIEWER_ACTION_OPEN_FILER -> ViewerAction.OPEN_FILER
    ViewerActionV1.VIEWER_ACTION_OPEN_SETTINGS -> ViewerAction.OPEN_SETTINGS
    ViewerActionV1.VIEWER_ACTION_OPEN_SUB_FILER -> ViewerAction.OPEN_SUBFILER
    ViewerActionV1.VIEWER_ACTION_TOGGLE_FIT -> ViewerAction.TOGGLE_FIT_MODE
    ViewerActionV1.VIEWER_ACTION_QUICK_MENU -> ViewerAction.OPEN_CONTEXT_MENU
    ViewerActionV1.VIEWER_ACTION_FIRST -> ViewerAction.FIRST_IMAGE
    ViewerActionV1.VIEWER_ACTION_LAST -> ViewerAction.LAST_IMAGE
    ViewerActionV1.VIEWER_ACTION_ZOOM_IN -> ViewerAction.ZOOM_IN
    ViewerActionV1.VIEWER_ACTION_ZOOM_OUT -> ViewerAction.ZOOM_OUT
    ViewerActionV1.VIEWER_ACTION_ZOOM_RESET -> ViewerAction.ZOOM_RESET
    ViewerActionV1.VIEWER_ACTION_TOGGLE_ANIMATION -> ViewerAction.TOGGLE_ANIMATION
    ViewerActionV1.VIEWER_ACTION_TOGGLE_GRAYSCALE -> ViewerAction.TOGGLE_GRAYSCALE
    ViewerActionV1.VIEWER_ACTION_TOGGLE_MANGA -> ViewerAction.TOGGLE_MANGA_MODE
    ViewerActionV1.VIEWER_ACTION_RELOAD -> ViewerAction.RELOAD
    ViewerActionV1.VIEWER_ACTION_EXPORT -> ViewerAction.EXPORT
    else -> fallback
}

private fun ViewerAction.toProto(): ViewerActionV1 = when (this) {
    ViewerAction.NONE -> ViewerActionV1.VIEWER_ACTION_DISABLED
    ViewerAction.PREVIOUS_IMAGE -> ViewerActionV1.VIEWER_ACTION_PREVIOUS
    ViewerAction.NEXT_IMAGE -> ViewerActionV1.VIEWER_ACTION_NEXT
    ViewerAction.OPEN_FILER -> ViewerActionV1.VIEWER_ACTION_OPEN_FILER
    ViewerAction.OPEN_SETTINGS -> ViewerActionV1.VIEWER_ACTION_OPEN_SETTINGS
    ViewerAction.OPEN_SUBFILER -> ViewerActionV1.VIEWER_ACTION_OPEN_SUB_FILER
    ViewerAction.TOGGLE_FIT_MODE -> ViewerActionV1.VIEWER_ACTION_TOGGLE_FIT
    ViewerAction.OPEN_CONTEXT_MENU -> ViewerActionV1.VIEWER_ACTION_QUICK_MENU
    ViewerAction.FIRST_IMAGE -> ViewerActionV1.VIEWER_ACTION_FIRST
    ViewerAction.LAST_IMAGE -> ViewerActionV1.VIEWER_ACTION_LAST
    ViewerAction.ZOOM_IN -> ViewerActionV1.VIEWER_ACTION_ZOOM_IN
    ViewerAction.ZOOM_OUT -> ViewerActionV1.VIEWER_ACTION_ZOOM_OUT
    ViewerAction.ZOOM_RESET -> ViewerActionV1.VIEWER_ACTION_ZOOM_RESET
    ViewerAction.TOGGLE_ANIMATION -> ViewerActionV1.VIEWER_ACTION_TOGGLE_ANIMATION
    ViewerAction.TOGGLE_GRAYSCALE -> ViewerActionV1.VIEWER_ACTION_TOGGLE_GRAYSCALE
    ViewerAction.TOGGLE_MANGA_MODE -> ViewerActionV1.VIEWER_ACTION_TOGGLE_MANGA
    ViewerAction.RELOAD -> ViewerActionV1.VIEWER_ACTION_RELOAD
    ViewerAction.EXPORT -> ViewerActionV1.VIEWER_ACTION_EXPORT
}

private fun DisplayFitV1.toUiDisplayFit() = when (this) {
    DisplayFitV1.DISPLAY_FIT_WIDTH -> DisplayFit.WIDTH
    DisplayFitV1.DISPLAY_FIT_HEIGHT -> DisplayFit.HEIGHT
    DisplayFitV1.DISPLAY_FIT_ORIGINAL -> DisplayFit.ORIGINAL
    else -> DisplayFit.CONTAIN
}
private fun DisplayFit.toProto() = when (this) {
    DisplayFit.CONTAIN -> DisplayFitV1.DISPLAY_FIT_CONTAIN
    DisplayFit.WIDTH -> DisplayFitV1.DISPLAY_FIT_WIDTH
    DisplayFit.HEIGHT -> DisplayFitV1.DISPLAY_FIT_HEIGHT
    DisplayFit.ORIGINAL -> DisplayFitV1.DISPLAY_FIT_ORIGINAL
}
private fun MangaLayoutV1.toUiMangaLayout() = when (this) {
    MangaLayoutV1.MANGA_LAYOUT_SINGLE -> MangaLayoutMode.SINGLE
    MangaLayoutV1.MANGA_LAYOUT_SPREAD -> MangaLayoutMode.SPREAD
    else -> MangaLayoutMode.AUTO
}
private fun MangaLayoutMode.toProto() = when (this) {
    MangaLayoutMode.AUTO -> MangaLayoutV1.MANGA_LAYOUT_AUTO
    MangaLayoutMode.SINGLE -> MangaLayoutV1.MANGA_LAYOUT_SINGLE
    MangaLayoutMode.SPREAD -> MangaLayoutV1.MANGA_LAYOUT_SPREAD
}
private fun ReadingDirectionV1.toUiReadingDirection() = if (this == ReadingDirectionV1.READING_DIRECTION_LTR) {
    ReadingDirection.LEFT_TO_RIGHT
} else ReadingDirection.RIGHT_TO_LEFT
private fun ReadingDirection.toProto() = if (this == ReadingDirection.LEFT_TO_RIGHT) {
    ReadingDirectionV1.READING_DIRECTION_LTR
} else ReadingDirectionV1.READING_DIRECTION_RTL
private fun SortOrderV1.toUiSortOrder() = when (this) {
    SortOrderV1.SORT_ORDER_NAME_DESCENDING -> FilerSortOrder.NAME_DESCENDING
    SortOrderV1.SORT_ORDER_MODIFIED_DESCENDING -> FilerSortOrder.MODIFIED_DESCENDING
    else -> FilerSortOrder.NAME_ASCENDING
}
private fun FilerSortOrder.toProto() = when (this) {
    FilerSortOrder.NAME_ASCENDING -> SortOrderV1.SORT_ORDER_NAME_ASCENDING
    FilerSortOrder.NAME_DESCENDING -> SortOrderV1.SORT_ORDER_NAME_DESCENDING
    FilerSortOrder.MODIFIED_DESCENDING -> SortOrderV1.SORT_ORDER_MODIFIED_DESCENDING
}
private fun CodecPolicyV1.toUiCodecPolicy(fallback: CodecPolicy = CodecPolicy.DEFAULT) = when (this) {
    CodecPolicyV1.CODEC_POLICY_INTERNAL_FIRST -> CodecPolicy.INTERNAL_FIRST
    CodecPolicyV1.CODEC_POLICY_OS_FIRST -> CodecPolicy.OS_FIRST
    CodecPolicyV1.CODEC_POLICY_INTERNAL_ONLY -> CodecPolicy.INTERNAL_ONLY
    CodecPolicyV1.CODEC_POLICY_OS_ONLY -> CodecPolicy.OS_ONLY
    CodecPolicyV1.CODEC_POLICY_DEFAULT -> CodecPolicy.DEFAULT
    else -> fallback
}
private fun CodecPolicy.toProto() = when (this) {
    CodecPolicy.DEFAULT -> CodecPolicyV1.CODEC_POLICY_DEFAULT
    CodecPolicy.INTERNAL_FIRST -> CodecPolicyV1.CODEC_POLICY_INTERNAL_FIRST
    CodecPolicy.OS_FIRST -> CodecPolicyV1.CODEC_POLICY_OS_FIRST
    CodecPolicy.INTERNAL_ONLY -> CodecPolicyV1.CODEC_POLICY_INTERNAL_ONLY
    CodecPolicy.OS_ONLY -> CodecPolicyV1.CODEC_POLICY_OS_ONLY
}
private fun ThemeModeV1.toUiTheme() = when (this) {
    ThemeModeV1.THEME_MODE_LIGHT -> ThemeMode.LIGHT
    ThemeModeV1.THEME_MODE_SYSTEM -> ThemeMode.SYSTEM
    else -> ThemeMode.CINEMATIC_DARK
}
private fun ThemeMode.toProto() = when (this) {
    ThemeMode.CINEMATIC_DARK -> ThemeModeV1.THEME_MODE_CINEMATIC_DARK
    ThemeMode.LIGHT -> ThemeModeV1.THEME_MODE_LIGHT
    ThemeMode.SYSTEM -> ThemeModeV1.THEME_MODE_SYSTEM
}
private fun TextScaleV1.toUiTextScale() = when (this) {
    TextScaleV1.TEXT_SCALE_SMALL -> TextScale.SMALL
    TextScaleV1.TEXT_SCALE_LARGE -> TextScale.LARGE
    else -> TextScale.MEDIUM
}
private fun TextScale.toProto() = when (this) {
    TextScale.SMALL -> TextScaleV1.TEXT_SCALE_SMALL
    TextScale.MEDIUM -> TextScaleV1.TEXT_SCALE_MEDIUM
    TextScale.LARGE -> TextScaleV1.TEXT_SCALE_LARGE
}

private const val MIB = 1024L * 1024L
