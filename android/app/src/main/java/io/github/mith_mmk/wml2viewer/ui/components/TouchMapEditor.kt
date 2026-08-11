package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.LayoutDirection
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.labelResource
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.TouchMapConfig
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction

@Composable
fun TouchMapEditor(
    config: TouchMapConfig,
    onChange: (TouchMapConfig) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = stringResource(R.string.touch_map_title),
            style = MaterialTheme.typography.titleMedium,
        )
        Text(
            text = stringResource(R.string.touch_map_summary),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
            repeat(3) { row ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    repeat(3) { column ->
                        val zone = TapZone.at(row, column)
                        TouchZonePicker(
                            zone = zone,
                            action = config.actionFor(zone),
                            onActionSelected = { onChange(config.withBinding(zone, it)) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
        TextButton(
            onClick = { onChange(TouchMapConfig()) },
            modifier = Modifier.testTag("touch-map-reset"),
        ) {
            Text(stringResource(R.string.touch_reset_default))
        }
    }
}

@Composable
private fun TouchZonePicker(
    zone: TapZone,
    action: ViewerAction,
    onActionSelected: (ViewerAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    var expanded by remember(zone) { mutableStateOf(false) }
    Box(modifier) {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier
                .fillMaxWidth()
                .testTag("touch-zone-${zone.name.lowercase()}"),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(
                horizontal = 6.dp,
                vertical = 8.dp,
            ),
        ) {
            Column {
                Text(
                    text = stringResource(zone.labelResource()),
                    style = MaterialTheme.typography.labelSmall,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    text = stringResource(action.labelResource()),
                    style = MaterialTheme.typography.bodySmall,
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            ViewerAction.entries.forEach { candidate ->
                DropdownMenuItem(
                    text = { Text(stringResource(candidate.labelResource())) },
                    onClick = {
                        expanded = false
                        onActionSelected(candidate)
                    },
                )
            }
        }
    }
}

@Composable
fun ViewerActionPicker(
    label: String,
    value: ViewerAction,
    onChange: (ViewerAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    var expanded by remember { mutableStateOf(false) }
    Box(modifier) {
        OutlinedButton(onClick = { expanded = true }, modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.fillMaxWidth()) {
                Text(label, style = MaterialTheme.typography.labelSmall)
                Text(stringResource(value.labelResource()))
            }
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            ViewerAction.entries.forEach { candidate ->
                DropdownMenuItem(
                    text = { Text(stringResource(candidate.labelResource())) },
                    onClick = {
                        expanded = false
                        onChange(candidate)
                    },
                )
            }
        }
    }
}
