package io.github.mith_mmk.wml2viewer.ui

import androidx.annotation.StringRes
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.FilerSortOrder
import io.github.mith_mmk.wml2viewer.ui.model.LanguagePreference
import io.github.mith_mmk.wml2viewer.ui.model.MangaLayoutMode
import io.github.mith_mmk.wml2viewer.ui.model.ReadingDirection
import io.github.mith_mmk.wml2viewer.ui.model.SettingsCategory
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.TextScale
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction

@StringRes
fun ViewerAction.labelResource(): Int = when (this) {
    ViewerAction.NONE -> R.string.action_none
    ViewerAction.PREVIOUS_IMAGE -> R.string.action_previous_image
    ViewerAction.NEXT_IMAGE -> R.string.action_next_image
    ViewerAction.FIRST_IMAGE -> R.string.action_first_image
    ViewerAction.LAST_IMAGE -> R.string.action_last_image
    ViewerAction.ZOOM_IN -> R.string.action_zoom_in
    ViewerAction.ZOOM_OUT -> R.string.action_zoom_out
    ViewerAction.ZOOM_RESET -> R.string.action_zoom_reset
    ViewerAction.TOGGLE_FIT_MODE -> R.string.action_toggle_fit
    ViewerAction.TOGGLE_ANIMATION -> R.string.action_toggle_animation
    ViewerAction.TOGGLE_GRAYSCALE -> R.string.action_toggle_grayscale
    ViewerAction.TOGGLE_MANGA_MODE -> R.string.action_toggle_manga
    ViewerAction.OPEN_FILER -> R.string.action_open_filer
    ViewerAction.OPEN_SETTINGS -> R.string.action_open_settings
    ViewerAction.OPEN_SUBFILER -> R.string.action_open_subfiler
    ViewerAction.OPEN_CONTEXT_MENU -> R.string.action_open_context_menu
    ViewerAction.EXPORT -> R.string.action_export
    ViewerAction.RELOAD -> R.string.action_reload
}

@StringRes
fun ExportFormat.labelResource(): Int = when (this) {
    ExportFormat.PNG -> R.string.export_format_png
    ExportFormat.JPEG -> R.string.export_format_jpeg
    ExportFormat.WEBP_LOSSY -> R.string.export_format_webp_lossy
    ExportFormat.WEBP_LOSSLESS -> R.string.export_format_webp_lossless
}

@StringRes
fun TapZone.labelResource(): Int = when (this) {
    TapZone.TOP_LEFT -> R.string.touch_zone_top_left
    TapZone.TOP_CENTER -> R.string.touch_zone_top_center
    TapZone.TOP_RIGHT -> R.string.touch_zone_top_right
    TapZone.MIDDLE_LEFT -> R.string.touch_zone_middle_left
    TapZone.CENTER -> R.string.touch_zone_center
    TapZone.MIDDLE_RIGHT -> R.string.touch_zone_middle_right
    TapZone.BOTTOM_LEFT -> R.string.touch_zone_bottom_left
    TapZone.BOTTOM_CENTER -> R.string.touch_zone_bottom_center
    TapZone.BOTTOM_RIGHT -> R.string.touch_zone_bottom_right
}

@StringRes
fun SettingsCategory.labelResource(): Int = when (this) {
    SettingsCategory.VIEWING -> R.string.settings_viewing
    SettingsCategory.MANGA -> R.string.settings_manga
    SettingsCategory.TOUCH -> R.string.settings_touch
    SettingsCategory.FILER -> R.string.settings_filer
    SettingsCategory.CODECS -> R.string.settings_codecs
    SettingsCategory.LANGUAGE_AND_APPEARANCE -> R.string.settings_language_appearance
    SettingsCategory.CACHE -> R.string.settings_cache
    SettingsCategory.ABOUT -> R.string.settings_about
}

@StringRes
fun MangaLayoutMode.labelResource(): Int = when (this) {
    MangaLayoutMode.AUTO -> R.string.manga_layout_auto
    MangaLayoutMode.SINGLE -> R.string.manga_layout_single
    MangaLayoutMode.SPREAD -> R.string.manga_layout_spread
}

@StringRes
fun ReadingDirection.labelResource(): Int = when (this) {
    ReadingDirection.RIGHT_TO_LEFT -> R.string.manga_right_to_left
    ReadingDirection.LEFT_TO_RIGHT -> R.string.manga_left_to_right
}

@StringRes
fun CodecPolicy.labelResource(): Int = when (this) {
    CodecPolicy.DEFAULT -> R.string.codec_default
    CodecPolicy.INTERNAL_FIRST -> R.string.codec_internal_first
    CodecPolicy.OS_FIRST -> R.string.codec_os_first
    CodecPolicy.INTERNAL_ONLY -> R.string.codec_internal_only
    CodecPolicy.OS_ONLY -> R.string.codec_os_only
}

@StringRes
fun CodecFormat.labelResource(): Int = when (this) {
    CodecFormat.JPEG -> R.string.codec_format_jpeg
    CodecFormat.PNG -> R.string.codec_format_png
    CodecFormat.GIF -> R.string.codec_format_gif
    CodecFormat.WEBP -> R.string.codec_format_webp
    CodecFormat.BMP -> R.string.codec_format_bmp
    CodecFormat.ICO -> R.string.codec_format_ico
    CodecFormat.HEIF -> R.string.codec_format_heif
    CodecFormat.AVIF -> R.string.codec_format_avif
    CodecFormat.DNG -> R.string.codec_format_dng
}

@StringRes
fun LanguagePreference.labelResource(): Int = when (this) {
    LanguagePreference.SYSTEM -> R.string.language_system
    LanguagePreference.ENGLISH -> R.string.language_english
    LanguagePreference.JAPANESE -> R.string.language_japanese
}

@StringRes
fun TextScale.labelResource(): Int = when (this) {
    TextScale.SMALL -> R.string.text_size_small
    TextScale.MEDIUM -> R.string.text_size_medium
    TextScale.LARGE -> R.string.text_size_large
}

@StringRes
fun ThemeMode.labelResource(): Int = when (this) {
    ThemeMode.CINEMATIC_DARK -> R.string.theme_cinematic_dark
    ThemeMode.LIGHT -> R.string.theme_light
    ThemeMode.SYSTEM -> R.string.theme_system
}

@StringRes
fun DisplayFit.labelResource(): Int = when (this) {
    DisplayFit.CONTAIN -> R.string.display_fit_contain
    DisplayFit.WIDTH -> R.string.display_fit_width
    DisplayFit.HEIGHT -> R.string.display_fit_height
    DisplayFit.ORIGINAL -> R.string.display_fit_original
}

@StringRes
fun FilerSortOrder.labelResource(): Int = when (this) {
    FilerSortOrder.NAME_ASCENDING -> R.string.filer_sort_name_ascending
    FilerSortOrder.NAME_DESCENDING -> R.string.filer_sort_name_descending
    FilerSortOrder.MODIFIED_DESCENDING -> R.string.filer_sort_modified_descending
}
