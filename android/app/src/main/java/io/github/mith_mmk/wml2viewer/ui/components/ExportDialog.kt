package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.labelResource
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import kotlin.math.roundToInt

@Composable
fun ExportDialog(
    supportedFormats: Set<ExportFormat>,
    initialTitle: String,
    canCreateCurrentDirectory: Boolean,
    exporting: Boolean,
    onConfirm: (ExportRequest) -> Unit,
    onDismiss: () -> Unit,
) {
    val formats = remember(supportedFormats) { ExportFormat.entries.filter(supportedFormats::contains) }
    var format by rememberSaveable { mutableStateOf(formats.firstOrNull() ?: ExportFormat.PNG) }
    var quality by rememberSaveable { mutableFloatStateOf(DEFAULT_EXPORT_QUALITY.toFloat()) }
    var fileName by rememberSaveable(initialTitle) {
        mutableStateOf(defaultExportFileName(initialTitle, format))
    }
    var destination by rememberSaveable(canCreateCurrentDirectory) {
        mutableStateOf(
            if (canCreateCurrentDirectory) ExportDestination.CURRENT_DIRECTORY
            else ExportDestination.SYSTEM_PICKER,
        )
    }
    LaunchedEffect(formats) {
        if (format !in formats) format = formats.firstOrNull() ?: ExportFormat.PNG
    }
    LaunchedEffect(format) {
        fileName = normalizeExportFileName(fileName, format)
            ?: defaultExportFileName(initialTitle, format)
    }
    LaunchedEffect(canCreateCurrentDirectory) {
        if (!canCreateCurrentDirectory) destination = ExportDestination.SYSTEM_PICKER
    }
    val normalizedName = normalizeExportFileName(fileName, format)

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.export_title)) },
        text = {
            Column(
                Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
            ) {
                if (formats.isEmpty()) {
                    Text(
                        text = stringResource(R.string.export_no_supported_formats),
                        color = MaterialTheme.colorScheme.error,
                    )
                } else {
                    Text(stringResource(R.string.export_format), style = MaterialTheme.typography.titleSmall)
                    formats.forEach { candidate ->
                        ExportChoiceRow(
                            label = stringResource(candidate.labelResource()),
                            selected = format == candidate,
                            enabled = !exporting,
                            onClick = { format = candidate },
                        )
                    }
                }
                OutlinedTextField(
                    value = fileName,
                    onValueChange = { fileName = it },
                    label = { Text(stringResource(R.string.export_file_name)) },
                    singleLine = true,
                    enabled = !exporting,
                    isError = fileName.isNotBlank() && normalizedName == null,
                    supportingText = if (fileName.isNotBlank() && normalizedName == null) {
                        { Text(stringResource(R.string.export_invalid_file_name)) }
                    } else null,
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                )
                if (format == ExportFormat.JPEG || format == ExportFormat.WEBP_LOSSY) {
                    Text(
                        text = stringResource(R.string.export_quality_value, quality.roundToInt()),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.padding(top = 12.dp),
                    )
                    Slider(
                        value = quality,
                        onValueChange = { quality = it },
                        valueRange = 1f..100f,
                        steps = 98,
                        enabled = !exporting,
                    )
                }
                Text(
                    text = stringResource(R.string.export_destination),
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(top = 8.dp),
                )
                ExportChoiceRow(
                    label = stringResource(R.string.export_destination_current_directory),
                    selected = destination == ExportDestination.CURRENT_DIRECTORY,
                    enabled = canCreateCurrentDirectory && !exporting,
                    onClick = { destination = ExportDestination.CURRENT_DIRECTORY },
                )
                if (!canCreateCurrentDirectory) {
                    Text(
                        text = stringResource(R.string.export_current_directory_unavailable),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                ExportChoiceRow(
                    label = stringResource(R.string.export_destination_system_picker),
                    selected = destination == ExportDestination.SYSTEM_PICKER,
                    enabled = !exporting,
                    onClick = { destination = ExportDestination.SYSTEM_PICKER },
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = formats.isNotEmpty() && normalizedName != null && !exporting,
                onClick = {
                    onConfirm(
                        ExportRequest(
                            format = format,
                            quality = quality.roundToInt(),
                            fileName = checkNotNull(normalizedName),
                            destination = destination,
                        ),
                    )
                },
            ) {
                Text(stringResource(if (exporting) R.string.exporting else R.string.export_save))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, enabled = !exporting) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

@Composable
private fun ExportChoiceRow(
    label: String,
    selected: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = onClick, enabled = enabled)
        Text(label)
    }
}

internal fun defaultExportFileName(title: String, format: ExportFormat): String {
    val sourceName = title.substringAfterLast('/').substringAfterLast('\\')
    val base = sourceName.substringBeforeLast('.', sourceName)
        .map { character ->
            if (character.code < 32 || character in InvalidExportNameCharacters) '_' else character
        }
        .joinToString("")
        .trim()
        .trimEnd('.')
        .ifBlank { DEFAULT_EXPORT_BASENAME }
    return "$base.${format.extension}"
}

internal fun normalizeExportFileName(value: String, format: ExportFormat): String? {
    val name = value.trim()
    if (name.isEmpty() || name.length > MAX_EXPORT_NAME_LENGTH || name == "." || name == "..") return null
    if (name.any { it.code < 32 || it in InvalidExportNameCharacters }) return null
    val extension = name.substringAfterLast('.', missingDelimiterValue = "").lowercase()
    val knownExtensions = ExportFormat.entries.mapTo(mutableSetOf()) { it.extension }
    val base = if (extension in knownExtensions) name.dropLast(extension.length + 1) else name
    val normalizedBase = base.trim().trimEnd('.')
    if (normalizedBase.isEmpty() || normalizedBase == "." || normalizedBase == "..") return null
    return "$normalizedBase.${format.extension}"
}

private val InvalidExportNameCharacters = setOf('\\', '/', ':', '*', '?', '"', '<', '>', '|')
private const val DEFAULT_EXPORT_QUALITY = 90
private const val DEFAULT_EXPORT_BASENAME = "export"
private const val MAX_EXPORT_NAME_LENGTH = 240
