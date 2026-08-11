package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.labelResource
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction

private val QuickMenuActions = listOf(
    ViewerAction.PREVIOUS_IMAGE,
    ViewerAction.NEXT_IMAGE,
    ViewerAction.TOGGLE_FIT_MODE,
    ViewerAction.TOGGLE_MANGA_MODE,
    ViewerAction.TOGGLE_ANIMATION,
    ViewerAction.EXPORT,
    ViewerAction.OPEN_FILER,
    ViewerAction.OPEN_SETTINGS,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ViewerQuickMenu(
    exportEnabled: Boolean,
    onAction: (ViewerAction) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        contentWindowInsets = { WindowInsets.navigationBars },
    ) {
        Column(Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
            Text(
                text = stringResource(R.string.quick_menu_title),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
            QuickMenuActions.forEach { action ->
                TextButton(
                    onClick = { onAction(action) },
                    enabled = action != ViewerAction.EXPORT || exportEnabled,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(stringResource(action.labelResource()))
                }
            }
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.close))
            }
        }
    }
}
