package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.PendingCollisionUi
import io.github.mith_mmk.wml2viewer.ui.model.PendingTransferUi
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput

@Composable
internal fun TransferDestinationBar(
    transfer: PendingTransferUi,
    destinationAvailable: Boolean,
    onCancel: () -> Unit,
    onComplete: () -> Unit,
) {
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
            Text(
                text = stringResource(R.string.filer_choose_destination, transfer.entryName),
                style = MaterialTheme.typography.bodyMedium,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onCancel) { Text(stringResource(R.string.close)) }
                TextButton(onClick = onComplete, enabled = destinationAvailable) {
                    Text(stringResource(R.string.filer_use_this_folder))
                }
            }
        }
    }
}

@Composable
internal fun FilerNameDialog(
    title: String,
    initialValue: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var name by remember(initialValue) { mutableStateOf(initialValue) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text(stringResource(R.string.filer_name)) },
                singleLine = true,
            )
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(name.trim()) },
                enabled = name.isNotBlank(),
            ) { Text(stringResource(R.string.filer_confirm)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

@Composable
internal fun FilerEntryActionsDialog(
    entry: FilerEntryUi,
    onDismiss: () -> Unit,
    onCopy: () -> Unit,
    onMove: () -> Unit,
    onRename: () -> Unit,
    onDelete: () -> Unit,
    onReenterCredential: () -> Unit,
    onForgetSource: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(entry.name) },
        text = {
            Column {
                if (entry.capabilities.canCopy) DialogAction(R.string.filer_copy, onCopy)
                if (entry.capabilities.canMove) DialogAction(R.string.filer_move, onMove)
                if (entry.capabilities.canRename) DialogAction(R.string.filer_rename, onRename)
                if (entry.capabilities.canTrash || entry.capabilities.canDeletePermanently) {
                    DialogAction(R.string.filer_delete, onDelete)
                }
                if (entry.credentialReentryRequired) {
                    DialogAction(R.string.smb_reenter_password, onReenterCredential)
                }
                if (entry.canForgetSource) {
                    DialogAction(R.string.smb_forget_source, onForgetSource)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

@Composable
internal fun SmbCredentialReentryDialog(
    sourceId: String,
    sourceName: String,
    onDismiss: () -> Unit,
    onConfirm: (SmbCredentialInput) -> Unit,
) {
    var password by remember(sourceId) { mutableStateOf("") }

    fun dismissAndClear() {
        password = ""
        onDismiss()
    }

    AlertDialog(
        onDismissRequest = ::dismissAndClear,
        title = { Text(stringResource(R.string.smb_reenter_password)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.smb_reenter_password_hint, sourceName))
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text(stringResource(R.string.smb_password)) },
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = password.isNotEmpty(),
                onClick = {
                    val input = SmbCredentialInput(sourceId, password.toCharArray())
                    password = ""
                    try {
                        onConfirm(input)
                    } finally {
                        input.clearPassword()
                    }
                    onDismiss()
                },
            ) { Text(stringResource(R.string.filer_confirm)) }
        },
        dismissButton = {
            TextButton(onClick = ::dismissAndClear) { Text(stringResource(R.string.close)) }
        },
    )
}

@Composable
internal fun SmbForgetSourceDialog(
    sourceName: String,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.smb_forget_source)) },
        text = { Text(stringResource(R.string.smb_forget_source_confirmation, sourceName)) },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.smb_forget_source)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

@Composable
private fun DialogAction(labelResource: Int, onClick: () -> Unit) {
    Text(
        text = stringResource(labelResource),
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 14.dp),
        style = MaterialTheme.typography.titleMedium,
    )
}

@Composable
internal fun FilerDeleteDialog(
    entry: FilerEntryUi,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    val permanent = !entry.capabilities.canTrash
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                stringResource(
                    if (permanent) R.string.filer_permanent_delete_title
                    else R.string.filer_delete_title,
                ),
            )
        },
        text = {
            Text(
                text = stringResource(
                    if (permanent) R.string.filer_permanent_delete_warning
                    else R.string.filer_trash_confirmation,
                    entry.name,
                ),
                color = if (permanent) MaterialTheme.colorScheme.error else Color.Unspecified,
            )
        },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(
                    stringResource(
                        if (permanent) R.string.filer_delete_permanently
                        else R.string.filer_move_to_trash,
                    ),
                    color = if (permanent) MaterialTheme.colorScheme.error else Color.Unspecified,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

@Composable
internal fun FilerCollisionDialog(
    collision: PendingCollisionUi,
    onResolve: (String, CollisionResolution, Boolean) -> Unit,
) {
    var applyToAll by remember(collision.operationId) { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = {},
        title = { Text(stringResource(R.string.filer_collision_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.filer_collision_message, collision.displayName))
                if (collision.supportsApplyToAll) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { applyToAll = !applyToAll },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(checked = applyToAll, onCheckedChange = { applyToAll = it })
                        Text(stringResource(R.string.filer_apply_to_all))
                    }
                }
            }
        },
        confirmButton = {
            Row {
                if (collision.allowReplace) {
                    TextButton(onClick = {
                        onResolve(collision.operationId, CollisionResolution.REPLACE, applyToAll)
                    }) { Text(stringResource(R.string.filer_collision_replace)) }
                }
                TextButton(onClick = {
                    onResolve(collision.operationId, CollisionResolution.KEEP_BOTH, applyToAll)
                }) { Text(stringResource(R.string.filer_collision_keep_both)) }
                TextButton(onClick = {
                    onResolve(collision.operationId, CollisionResolution.SKIP, applyToAll)
                }) { Text(stringResource(R.string.filer_collision_skip)) }
            }
        },
    )
}

private enum class SmbStep { SERVER, AUTHENTICATION, SHARE }

@Composable
internal fun SmbSetupDialog(
    availableShares: List<String>,
    loadingShares: Boolean,
    setupId: String?,
    onRequestShares: (SmbConnectionInput) -> Unit,
    onAddSource: (SmbConnectionInput) -> Unit,
    onDismiss: () -> Unit,
) {
    var step by remember { mutableStateOf(SmbStep.SERVER) }
    var server by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("445") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var domain by remember { mutableStateOf("") }
    var guest by remember { mutableStateOf(false) }
    var requireEncryption by remember { mutableStateOf(false) }
    var share by remember { mutableStateOf("") }

    fun dismissAndClear() {
        password = ""
        onDismiss()
    }

    AlertDialog(
        onDismissRequest = ::dismissAndClear,
        title = {
            Text(
                stringResource(
                    when (step) {
                        SmbStep.SERVER -> R.string.smb_server_title
                        SmbStep.AUTHENTICATION -> R.string.smb_authentication_title
                        SmbStep.SHARE -> R.string.smb_share_title
                    },
                ),
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                when (step) {
                    SmbStep.SERVER -> {
                        OutlinedTextField(
                            value = server,
                            onValueChange = { server = it },
                            label = { Text(stringResource(R.string.smb_server)) },
                            singleLine = true,
                        )
                        OutlinedTextField(
                            value = port,
                            onValueChange = { port = it.filter(Char::isDigit) },
                            label = { Text(stringResource(R.string.smb_port)) },
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                            singleLine = true,
                        )
                    }
                    SmbStep.AUTHENTICATION -> {
                        ToggleRow(
                            label = stringResource(R.string.smb_guest),
                            checked = guest,
                            onChange = { enabled ->
                                guest = enabled
                                if (enabled) password = ""
                            },
                        )
                        if (!guest) {
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text(stringResource(R.string.smb_username)) },
                                singleLine = true,
                            )
                            OutlinedTextField(
                                value = password,
                                onValueChange = { password = it },
                                label = { Text(stringResource(R.string.smb_password)) },
                                visualTransformation = PasswordVisualTransformation(),
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                                singleLine = true,
                            )
                            OutlinedTextField(
                                value = domain,
                                onValueChange = { domain = it },
                                label = { Text(stringResource(R.string.smb_domain)) },
                                singleLine = true,
                            )
                        }
                        ToggleRow(
                            label = stringResource(R.string.smb_require_encryption),
                            checked = requireEncryption,
                            onChange = { requireEncryption = it },
                        )
                        if (!requireEncryption) {
                            Text(
                                text = stringResource(R.string.smb_unencrypted_warning),
                                color = MaterialTheme.colorScheme.error,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    SmbStep.SHARE -> {
                        if (loadingShares) CircularProgressIndicator()
                        availableShares.forEach { available ->
                            Text(
                                text = available,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable { share = available }
                                    .padding(vertical = 8.dp),
                                color = if (share == available) MaterialTheme.colorScheme.primary
                                else Color.Unspecified,
                            )
                        }
                        OutlinedTextField(
                            value = share,
                            onValueChange = { share = it },
                            label = { Text(stringResource(R.string.smb_share_manual)) },
                            singleLine = true,
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = when (step) {
                    SmbStep.SERVER -> server.isNotBlank() &&
                        (port.toIntOrNull() ?: 0) in 1..65535
                    SmbStep.AUTHENTICATION -> guest || (username.isNotBlank() && password.isNotEmpty())
                    SmbStep.SHARE -> share.isNotBlank() && setupId != null && !loadingShares &&
                        (guest || password.isNotEmpty())
                },
                onClick = {
                    when (step) {
                        SmbStep.SERVER -> step = SmbStep.AUTHENTICATION
                        SmbStep.AUTHENTICATION -> {
                            val input = buildSmbConnectionInput(
                                server = server,
                                port = port.toInt(),
                                username = username,
                                password = password,
                                domain = domain,
                                guest = guest,
                                requireEncryption = requireEncryption,
                            )
                            try {
                                onRequestShares(input)
                            } finally {
                                input.clearPassword()
                            }
                            step = SmbStep.SHARE
                        }
                        SmbStep.SHARE -> {
                            val input = buildSmbConnectionInput(
                                server = server,
                                port = port.toInt(),
                                share = share,
                                username = username,
                                password = password,
                                domain = domain,
                                guest = guest,
                                requireEncryption = requireEncryption,
                                setupId = setupId,
                            )
                            password = ""
                            try {
                                onAddSource(input)
                            } finally {
                                input.clearPassword()
                            }
                            dismissAndClear()
                        }
                    }
                },
            ) {
                Text(
                    stringResource(
                        if (step == SmbStep.SHARE) R.string.smb_add_source
                        else R.string.smb_next,
                    ),
                )
            }
        },
        dismissButton = {
            TextButton(onClick = ::dismissAndClear) { Text(stringResource(R.string.close)) }
        },
    )
}

internal fun buildSmbConnectionInput(
    server: String,
    port: Int,
    share: String = "",
    username: String = "",
    password: String = "",
    domain: String = "",
    guest: Boolean,
    requireEncryption: Boolean,
    setupId: String? = null,
): SmbConnectionInput = SmbConnectionInput(
    server = server.trim(),
    port = port,
    share = share.trim(),
    username = if (guest) "" else username,
    password = if (guest) charArrayOf() else password.toCharArray(),
    domain = if (guest) "" else domain,
    guest = guest,
    requireEncryption = requireEncryption,
    setupId = setupId,
)

@Composable
private fun ToggleRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onChange(!checked) },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}
