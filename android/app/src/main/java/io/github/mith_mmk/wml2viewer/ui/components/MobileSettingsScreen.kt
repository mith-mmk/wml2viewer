package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.labelResource
import io.github.mith_mmk.wml2viewer.ui.model.CodecFormat
import io.github.mith_mmk.wml2viewer.ui.model.CodecPolicy
import io.github.mith_mmk.wml2viewer.ui.model.DisplayFit
import io.github.mith_mmk.wml2viewer.ui.model.FilerSortOrder
import io.github.mith_mmk.wml2viewer.ui.model.LanguagePreference
import io.github.mith_mmk.wml2viewer.ui.model.MangaLayoutMode
import io.github.mith_mmk.wml2viewer.ui.model.MobileViewerSettings
import io.github.mith_mmk.wml2viewer.ui.model.ReadingDirection
import io.github.mith_mmk.wml2viewer.ui.model.SettingsCategory
import io.github.mith_mmk.wml2viewer.ui.model.TextScale
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode
import kotlin.math.roundToInt

@Composable
fun MobileSettingsScreen(
    settings: MobileViewerSettings,
    measuredOsCodecFormats: Set<CodecFormat> = emptySet(),
    selectedCategory: SettingsCategory?,
    compact: Boolean,
    onBack: () -> Unit,
    onSelectCategory: (SettingsCategory?) -> Unit,
    onSettingsChange: (MobileViewerSettings) -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier
            .fillMaxSize()
            .testTag("settings-screen"),
        color = MaterialTheme.colorScheme.background,
    ) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            SettingsHeader(onBack)
            if (compact) {
                if (selectedCategory == null) {
                    CategoryList(selectedCategory, onSelectCategory)
                } else {
                    SettingsDetail(
                        selectedCategory,
                        settings,
                        measuredOsCodecFormats,
                        onSettingsChange,
                    )
                }
            } else {
                Row(Modifier.fillMaxSize()) {
                    CategoryList(
                        selected = selectedCategory ?: SettingsCategory.VIEWING,
                        onSelect = onSelectCategory,
                        modifier = Modifier.width(232.dp),
                    )
                    VerticalDivider()
                    SettingsDetail(
                        category = selectedCategory ?: SettingsCategory.VIEWING,
                        settings = settings,
                        measuredOsCodecFormats = measuredOsCodecFormats,
                        onSettingsChange = onSettingsChange,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

@Composable
private fun SettingsHeader(onBack: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextButton(onClick = onBack) { Text(stringResource(R.string.navigation_back)) }
        Text(
            text = stringResource(R.string.settings_title),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = stringResource(R.string.settings_changes_saved_immediately),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    HorizontalDivider()
}

@Composable
private fun CategoryList(
    selected: SettingsCategory?,
    onSelect: (SettingsCategory?) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(modifier.fillMaxSize().testTag("settings-categories")) {
        items(SettingsCategory.entries, key = { it.name }) { category ->
            Surface(
                color = if (category == selected) MaterialTheme.colorScheme.primaryContainer
                else MaterialTheme.colorScheme.surface,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onSelect(category) }
                    .testTag("settings-category-${category.name.lowercase()}"),
            ) {
                Text(
                    text = stringResource(category.labelResource()),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 18.dp),
                )
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.2f))
        }
    }
}

@Composable
private fun SettingsDetail(
    category: SettingsCategory,
    settings: MobileViewerSettings,
    measuredOsCodecFormats: Set<CodecFormat>,
    onSettingsChange: (MobileViewerSettings) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .testTag("settings-detail-${category.name.lowercase()}"),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Text(
                text = stringResource(category.labelResource()),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp),
            )
        }
        when (category) {
            SettingsCategory.VIEWING -> item {
                ViewingSettings(settings, onSettingsChange)
            }
            SettingsCategory.MANGA -> item {
                MangaSettings(settings, onSettingsChange)
            }
            SettingsCategory.TOUCH -> item {
                TouchSettings(settings, onSettingsChange)
            }
            SettingsCategory.FILER -> item {
                FilerSettings(settings, onSettingsChange)
            }
            SettingsCategory.CODECS -> item {
                CodecSettings(settings, measuredOsCodecFormats, onSettingsChange)
            }
            SettingsCategory.LANGUAGE_AND_APPEARANCE -> item {
                LanguageAppearanceSettings(settings, onSettingsChange)
            }
            SettingsCategory.CACHE -> item {
                CacheSettings(settings, onSettingsChange)
            }
            SettingsCategory.ABOUT -> item { AboutSettings() }
        }
        item { Spacer(Modifier.height(20.dp)) }
    }
}

@Composable
private fun ViewingSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        SettingSwitch(
            label = stringResource(R.string.viewing_edge_to_edge),
            checked = settings.viewing.edgeToEdge,
            onCheckedChange = {
                onChange(settings.copy(viewing = settings.viewing.copy(edgeToEdge = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.viewing_keep_screen_on),
            checked = settings.viewing.keepScreenOn,
            onCheckedChange = {
                onChange(settings.copy(viewing = settings.viewing.copy(keepScreenOn = it)))
            },
        )
        ChoiceTitle(stringResource(R.string.viewing_display_fit))
        DisplayFit.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.viewing.fit == value,
                onClick = { onChange(settings.copy(viewing = settings.viewing.copy(fit = value))) },
            )
        }
        SettingSwitch(
            label = stringResource(R.string.viewing_show_top_chrome),
            checked = settings.viewing.showTopChrome,
            onCheckedChange = {
                onChange(settings.copy(viewing = settings.viewing.copy(showTopChrome = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.viewing_show_filmstrip),
            checked = settings.viewing.showFilmstrip,
            onCheckedChange = {
                onChange(settings.copy(viewing = settings.viewing.copy(showFilmstrip = it)))
            },
        )
    }
}

@Composable
private fun MangaSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        ChoiceTitle(stringResource(R.string.manga_landscape_layout))
        MangaLayoutMode.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.manga.layoutMode == value,
                onClick = { onChange(settings.copy(manga = settings.manga.copy(layoutMode = value))) },
            )
        }
        ChoiceTitle(stringResource(R.string.manga_reading_direction))
        ReadingDirection.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.manga.readingDirection == value,
                onClick = {
                    onChange(settings.copy(manga = settings.manga.copy(readingDirection = value)))
                },
            )
        }
        SettingSwitch(
            label = stringResource(R.string.manga_single_cover),
            checked = settings.manga.singleCover,
            onCheckedChange = {
                onChange(settings.copy(manga = settings.manga.copy(singleCover = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.manga_divider),
            checked = settings.manga.divider,
            onCheckedChange = {
                onChange(settings.copy(manga = settings.manga.copy(divider = it)))
            },
        )
        ChoiceTitle(stringResource(R.string.manga_prefetch))
        Text(
            pluralStringResource(
                R.plurals.manga_prefetch_value,
                settings.manga.prefetchSpreads,
                settings.manga.prefetchSpreads,
            ),
        )
        Slider(
            value = settings.manga.prefetchSpreads.toFloat(),
            onValueChange = {
                onChange(
                    settings.copy(
                        manga = settings.manga.copy(prefetchSpreads = it.roundToInt()),
                    ),
                )
            },
            valueRange = 0f..1f,
            steps = 0,
        )
        InfoText(stringResource(R.string.manga_source_boundary_note))
    }
}

@Composable
private fun TouchSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        TouchMapEditor(
            config = settings.touchMap,
            onChange = { onChange(settings.copy(touchMap = it)) },
        )
        HorizontalDivider()
        ChoiceTitle(stringResource(R.string.touch_gestures))
        SettingSwitch(
            label = stringResource(R.string.touch_swipe),
            checked = settings.gestures.swipeEnabled,
            onCheckedChange = {
                onChange(settings.copy(gestures = settings.gestures.copy(swipeEnabled = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.touch_pinch_zoom),
            checked = settings.gestures.pinchZoom,
            onCheckedChange = {
                onChange(settings.copy(gestures = settings.gestures.copy(pinchZoom = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.touch_pan),
            checked = settings.gestures.pan,
            onCheckedChange = {
                onChange(settings.copy(gestures = settings.gestures.copy(pan = it)))
            },
        )
        ViewerActionPicker(
            label = stringResource(R.string.touch_double_tap),
            value = settings.gestures.doubleTapAction,
            onChange = {
                onChange(settings.copy(gestures = settings.gestures.copy(doubleTapAction = it)))
            },
        )
        ViewerActionPicker(
            label = stringResource(R.string.touch_long_press),
            value = settings.gestures.longPressAction,
            onChange = {
                onChange(settings.copy(gestures = settings.gestures.copy(longPressAction = it)))
            },
        )
    }
}

@Composable
private fun FilerSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        SettingSwitch(
            label = stringResource(R.string.filer_tablet_filmstrip_pinned),
            checked = settings.filer.tabletFilmstripPinned,
            onCheckedChange = {
                onChange(
                    settings.copy(filer = settings.filer.copy(tabletFilmstripPinned = it)),
                )
            },
        )
        SettingSwitch(
            label = stringResource(R.string.filer_show_hidden_files),
            checked = settings.filer.showHiddenFiles,
            onCheckedChange = {
                onChange(settings.copy(filer = settings.filer.copy(showHiddenFiles = it)))
            },
        )
        SettingSwitch(
            label = stringResource(R.string.filer_remember_last_location),
            checked = settings.filer.rememberLastLocation,
            onCheckedChange = {
                onChange(settings.copy(filer = settings.filer.copy(rememberLastLocation = it)))
            },
        )
        ChoiceTitle(stringResource(R.string.filer_sort_order))
        FilerSortOrder.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.filer.sortOrder == value,
                onClick = {
                    onChange(settings.copy(filer = settings.filer.copy(sortOrder = value)))
                },
            )
        }
        InfoText(stringResource(R.string.filer_smb_managed_elsewhere))
    }
}

@Composable
private fun CodecSettings(
    settings: MobileViewerSettings,
    measuredOsCodecFormats: Set<CodecFormat>,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        ChoiceTitle(stringResource(R.string.codec_os_probe_title))
        val measuredFormats = orderedMeasuredCodecFormats(measuredOsCodecFormats)
        if (measuredFormats.isEmpty()) {
            InfoText(stringResource(R.string.codec_os_probe_none))
        } else {
            val labels = measuredFormats.map { stringResource(it.labelResource()) }.joinToString(", ")
            InfoText(stringResource(R.string.codec_os_probe_passed, labels))
        }
        ChoiceTitle(stringResource(R.string.codec_preference))
        CodecPolicy.entries.filterNot { it == CodecPolicy.DEFAULT }.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.codecs.defaultPolicy == value,
                onClick = {
                    onChange(settings.copy(codecs = settings.codecs.copy(defaultPolicy = value)))
                },
            )
        }
        ChoiceTitle(stringResource(R.string.codec_per_format))
        CodecFormat.entries.forEach { format ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(format.labelResource()), modifier = Modifier.weight(1f))
                CodecPolicyPicker(
                    policy = settings.codecs.policyFor(format),
                    osSupported = format in measuredOsCodecFormats,
                    onChange = {
                        onChange(settings.copy(codecs = settings.codecs.withOverride(format, it)))
                    },
                )
            }
        }
        InfoText(stringResource(R.string.codec_note))
    }
}

internal fun orderedMeasuredCodecFormats(measured: Set<CodecFormat>): List<CodecFormat> =
    CodecFormat.entries.filter(measured::contains)

@Composable
private fun LanguageAppearanceSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        ChoiceTitle(stringResource(R.string.language_title))
        LanguagePreference.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.language == value,
                onClick = { onChange(settings.copy(language = value)) },
            )
        }
        ChoiceTitle(stringResource(R.string.appearance_title))
        ThemeMode.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.theme == value,
                onClick = { onChange(settings.copy(theme = value)) },
            )
        }
        SettingSwitch(
            label = stringResource(R.string.appearance_dynamic_color),
            checked = settings.dynamicColor,
            onCheckedChange = { onChange(settings.copy(dynamicColor = it)) },
        )
        if (settings.theme == ThemeMode.CINEMATIC_DARK) {
            InfoText(stringResource(R.string.appearance_cinematic_dark_summary))
        }
        ChoiceTitle(stringResource(R.string.text_size))
        TextScale.entries.forEach { value ->
            ChoiceRow(
                label = stringResource(value.labelResource()),
                selected = settings.textScale == value,
                onClick = { onChange(settings.copy(textScale = value)) },
            )
        }
    }
}

@Composable
private fun CacheSettings(
    settings: MobileViewerSettings,
    onChange: (MobileViewerSettings) -> Unit,
) {
    SettingsSection {
        SettingSwitch(
            label = stringResource(R.string.cache_automatic_limit),
            checked = settings.cache.automaticLimit,
            onCheckedChange = {
                onChange(settings.copy(cache = settings.cache.copy(automaticLimit = it)))
            },
        )
        if (!settings.cache.automaticLimit) {
            ChoiceTitle(stringResource(R.string.cache_manual_limit))
            Text(stringResource(R.string.cache_manual_limit_value, settings.cache.manualLimitMiB))
            Slider(
                value = settings.cache.manualLimitMiB.toFloat(),
                onValueChange = {
                    val value = (it / 256f).roundToInt() * 256
                    onChange(settings.copy(cache = settings.cache.copy(manualLimitMiB = value)))
                },
                valueRange = 256f..2048f,
                steps = 6,
            )
        }
        InfoText(stringResource(R.string.cache_policy_note))
    }
}

@Composable
private fun CodecPolicyPicker(
    policy: CodecPolicy,
    osSupported: Boolean = true,
    onChange: (CodecPolicy) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val selectedLabel = stringResource(policy.labelResource())
    val selectedUnavailable = !osSupported && policy in OS_ONLY_POLICIES
    Box {
        OutlinedButton(onClick = { expanded = true }) {
            Text(
                if (selectedUnavailable) {
                    stringResource(R.string.codec_policy_unavailable, selectedLabel)
                } else {
                    selectedLabel
                },
            )
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            selectableCodecPolicies(osSupported).forEach { value ->
                DropdownMenuItem(
                    text = { Text(stringResource(value.labelResource())) },
                    onClick = {
                        expanded = false
                        onChange(value)
                    },
                )
            }
        }
    }
}

internal fun selectableCodecPolicies(osSupported: Boolean): List<CodecPolicy> =
    if (osSupported) CodecPolicy.entries else CodecPolicy.entries.filterNot(OS_ONLY_POLICIES::contains)

private val OS_ONLY_POLICIES = setOf(CodecPolicy.OS_FIRST, CodecPolicy.OS_ONLY)

@Composable
private fun AboutSettings() {
    SettingsSection {
        Text(
            text = stringResource(R.string.about_program),
            style = MaterialTheme.typography.titleMedium,
        )
        InfoText(stringResource(R.string.about_summary))
        InfoText(stringResource(R.string.about_android_ios_note))
    }
}

@Composable
private fun SettingsSection(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        content = content,
    )
}

@Composable
private fun SettingSwitch(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {}
            .toggleable(
                value = checked,
                role = Role.Switch,
                onValueChange = onCheckedChange,
            )
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = label, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = null)
    }
}

@Composable
private fun ChoiceTitle(label: String) {
    Text(
        text = label,
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 6.dp),
    )
}

@Composable
private fun ChoiceRow(label: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {}
            .selectable(
                selected = selected,
                role = Role.RadioButton,
                onClick = onClick,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = null)
        Text(label)
    }
}

@Composable
private fun InfoText(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
