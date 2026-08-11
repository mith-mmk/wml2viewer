package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.model.FilerEntryUi
import io.github.mith_mmk.wml2viewer.ui.model.BreadcrumbUi
import io.github.mith_mmk.wml2viewer.ui.model.CollisionResolution
import io.github.mith_mmk.wml2viewer.ui.model.FilerCapabilitiesUi
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationRequest
import io.github.mith_mmk.wml2viewer.ui.model.FilerOperationType
import io.github.mith_mmk.wml2viewer.ui.model.PendingCollisionUi
import io.github.mith_mmk.wml2viewer.ui.model.PendingTransferUi
import io.github.mith_mmk.wml2viewer.ui.model.SmbConnectionInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import io.github.mith_mmk.wml2viewer.ui.model.SmbSecurityStatusUi
import io.github.mith_mmk.wml2viewer.ui.model.SourceKind

@Composable
fun FilerPane(
    entries: List<FilerEntryUi>,
    selectedSource: SourceKind,
    pathLabel: String,
    currentDirectoryId: String?,
    breadcrumb: List<BreadcrumbUi>,
    currentCapabilities: FilerCapabilitiesUi,
    pendingTransfer: PendingTransferUi?,
    pendingCollision: PendingCollisionUi?,
    availableSmbShares: List<String>,
    smbSharesLoading: Boolean,
    smbSetupId: String?,
    smbSecurityStatus: SmbSecurityStatusUi?,
    compact: Boolean,
    onBack: () -> Unit,
    onSelectSource: (SourceKind) -> Unit,
    onRefresh: () -> Unit,
    onSelectEntry: (FilerEntryUi) -> Unit,
    onNavigateToBreadcrumb: (String) -> Unit,
    onRequestSafRoot: () -> Unit,
    onRequestSmbShares: (SmbConnectionInput) -> Unit,
    onAddSmbSource: (SmbConnectionInput) -> Unit,
    onReenterSmbCredential: (SmbCredentialInput) -> Unit,
    onForgetSmbSource: (String) -> Unit,
    onBeginTransfer: (PendingTransferUi) -> Unit,
    onCancelTransfer: () -> Unit,
    onCompleteTransfer: () -> Unit,
    onOperation: (FilerOperationRequest) -> Unit,
    onResolveCollision: (String, CollisionResolution, Boolean) -> Unit,
    listState: LazyListState = rememberLazyListState(),
    navigationControls: Boolean = true,
    paneTag: String = "filer-pane",
    modifier: Modifier = Modifier,
) {
    var actionEntry by remember { mutableStateOf<FilerEntryUi?>(null) }
    var renameEntry by remember { mutableStateOf<FilerEntryUi?>(null) }
    var deleteEntry by remember { mutableStateOf<FilerEntryUi?>(null) }
    var creatingFolder by remember { mutableStateOf(false) }
    var showingSmbSetup by remember { mutableStateOf(false) }
    var credentialEntry by remember { mutableStateOf<FilerEntryUi?>(null) }
    var forgetSourceEntry by remember { mutableStateOf<FilerEntryUi?>(null) }
    Surface(
        modifier = modifier
            .fillMaxSize()
            .testTag(paneTag),
        color = MaterialTheme.colorScheme.surface,
    ) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (compact) {
                    TextButton(onClick = onBack) {
                        Text(stringResource(R.string.navigation_back))
                    }
                }
                Text(
                    text = stringResource(R.string.filer_title),
                    style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = onRefresh) {
                    Text(stringResource(R.string.filer_refresh))
                }
            }
            if (navigationControls) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.padding(horizontal = 16.dp),
                ) {
                    SourceKind.entries.forEach { source ->
                        FilterChip(
                            selected = source == selectedSource,
                            onClick = { onSelectSource(source) },
                            label = {
                                Text(
                                    stringResource(
                                        if (source == SourceKind.LOCAL) R.string.filer_local
                                        else R.string.filer_smb,
                                    ),
                                )
                            },
                        )
                    }
                }
                if (breadcrumb.isNotEmpty()) {
                    LazyRow(
                        horizontalArrangement = Arrangement.spacedBy(2.dp),
                        modifier = Modifier.padding(horizontal = 8.dp),
                    ) {
                        items(breadcrumb, key = { it.id }) { item ->
                            TextButton(onClick = { onNavigateToBreadcrumb(item.id) }) {
                                Text(item.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            if (item != breadcrumb.last()) Text("/", modifier = Modifier.padding(top = 12.dp))
                        }
                    }
                } else {
                    FilerPathLabel(pathLabel, selectedSource)
                }
                Row(
                    modifier = Modifier
                        .horizontalScroll(rememberScrollState())
                        .padding(horizontal = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    TextButton(onClick = onRequestSafRoot) {
                        Text(
                            stringResource(R.string.filer_add_saf_root),
                            maxLines = 1,
                            softWrap = false,
                        )
                    }
                    TextButton(onClick = { showingSmbSetup = true }) {
                        Text(
                            stringResource(R.string.filer_add_smb),
                            maxLines = 1,
                            softWrap = false,
                        )
                    }
                    if (currentCapabilities.canCreate && currentDirectoryId != null) {
                        TextButton(onClick = { creatingFolder = true }) {
                            Text(
                                stringResource(R.string.filer_create_folder),
                                maxLines = 1,
                                softWrap = false,
                            )
                        }
                    }
                }
            } else {
                FilerPathLabel(pathLabel, selectedSource)
            }
            pendingTransfer?.let { transfer ->
                TransferDestinationBar(
                    transfer = transfer,
                    destinationAvailable = currentDirectoryId != null && currentCapabilities.canCreate,
                    onCancel = onCancelTransfer,
                    onComplete = onCompleteTransfer,
                )
            }
            if (selectedSource == SourceKind.SMB && smbSecurityStatus != null) {
                SmbSecurityBanner(smbSecurityStatus)
            }
            Text(
                text = stringResource(
                    if (compact) R.string.filer_compact_description
                    else R.string.filer_expanded_description,
                ),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            Spacer(Modifier.height(8.dp))
            HorizontalDivider()
            if (entries.isEmpty()) {
                Text(
                    text = stringResource(R.string.filer_no_entries),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 20.dp),
                )
            } else {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .fillMaxSize()
                        .testTag("$paneTag-list"),
                ) {
                    items(entries, key = { it.id }) { entry ->
                        FilerEntryRow(
                            entry = entry,
                            onSelectEntry = onSelectEntry,
                            onMore = { actionEntry = entry },
                        )
                    }
                }
            }
        }
    }

    actionEntry?.let { entry ->
        FilerEntryActionsDialog(
            entry = entry,
            onDismiss = { actionEntry = null },
            onCopy = {
                actionEntry = null
                onBeginTransfer(PendingTransferUi(FilerOperationType.COPY, entry.id, entry.name))
            },
            onMove = {
                actionEntry = null
                onBeginTransfer(PendingTransferUi(FilerOperationType.MOVE, entry.id, entry.name))
            },
            onRename = {
                actionEntry = null
                renameEntry = entry
            },
            onDelete = {
                actionEntry = null
                deleteEntry = entry
            },
            onReenterCredential = {
                actionEntry = null
                credentialEntry = entry
            },
            onForgetSource = {
                actionEntry = null
                forgetSourceEntry = entry
            },
        )
    }
    if (creatingFolder) {
        FilerNameDialog(
            title = stringResource(R.string.filer_create_folder),
            initialValue = "",
            onDismiss = { creatingFolder = false },
            onConfirm = { name ->
                creatingFolder = false
                onOperation(
                    FilerOperationRequest(
                        type = FilerOperationType.CREATE_FOLDER,
                        destinationId = currentDirectoryId,
                        name = name,
                    ),
                )
            },
        )
    }
    renameEntry?.let { entry ->
        FilerNameDialog(
            title = stringResource(R.string.filer_rename),
            initialValue = entry.name,
            onDismiss = { renameEntry = null },
            onConfirm = { name ->
                renameEntry = null
                onOperation(
                    FilerOperationRequest(
                        type = FilerOperationType.RENAME,
                        entryId = entry.id,
                        name = name,
                    ),
                )
            },
        )
    }
    deleteEntry?.let { entry ->
        FilerDeleteDialog(
            entry = entry,
            onDismiss = { deleteEntry = null },
            onConfirm = {
                deleteEntry = null
                onOperation(
                    FilerOperationRequest(
                        type = FilerOperationType.DELETE,
                        entryId = entry.id,
                        allowPermanentDelete = !entry.capabilities.canTrash,
                    ),
                )
            },
        )
    }
    if (showingSmbSetup) {
        SmbSetupDialog(
            availableShares = availableSmbShares,
            loadingShares = smbSharesLoading,
            setupId = smbSetupId,
            onRequestShares = onRequestSmbShares,
            onAddSource = onAddSmbSource,
            onDismiss = { showingSmbSetup = false },
        )
    }
    credentialEntry?.let { entry ->
        entry.managedSourceId?.let { sourceId ->
            SmbCredentialReentryDialog(
                sourceId = sourceId,
                sourceName = entry.name,
                onDismiss = { credentialEntry = null },
                onConfirm = { input ->
                    credentialEntry = null
                    onReenterSmbCredential(input)
                },
            )
        }
    }
    forgetSourceEntry?.let { entry ->
        entry.managedSourceId?.let { sourceId ->
            SmbForgetSourceDialog(
                sourceName = entry.name,
                onDismiss = { forgetSourceEntry = null },
                onConfirm = {
                    forgetSourceEntry = null
                    onForgetSmbSource(sourceId)
                },
            )
        }
    }
    pendingCollision?.let { collision ->
        FilerCollisionDialog(
            collision = collision,
            onResolve = onResolveCollision,
        )
    }
}

@Composable
private fun SmbSecurityBanner(status: SmbSecurityStatusUi) {
    val warning = status.signingActive == false || status.encryptionActive == false
    Surface(
        color = if (warning) MaterialTheme.colorScheme.errorContainer
        else MaterialTheme.colorScheme.secondaryContainer,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
            status.dialect?.let {
                Text(stringResource(R.string.smb_security_dialect, it))
            }
            if (status.signingActive == false) {
                Text(
                    stringResource(R.string.smb_security_unsigned_warning),
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
            }
            if (status.encryptionActive == false) {
                Text(
                    stringResource(R.string.smb_security_unencrypted_warning),
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
            }
        }
    }
}

@Composable
private fun FilerPathLabel(pathLabel: String, selectedSource: SourceKind) {
    Text(
        text = pathLabel.ifBlank {
            stringResource(
                if (selectedSource == SourceKind.LOCAL) R.string.filer_local
                else R.string.filer_smb,
            )
        },
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
    )
}

@Composable
private fun FilerEntryRow(
    entry: FilerEntryUi,
    onSelectEntry: (FilerEntryUi) -> Unit,
    onMore: () -> Unit,
) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(
            modifier = Modifier
                .weight(1f)
                .clickable { onSelectEntry(entry) }
                .testTag("filer-entry-${entry.id}")
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            Text(
                text = entry.name,
                fontWeight = if (entry.isContainer) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = when {
                    entry.credentialReentryRequired -> stringResource(R.string.smb_credentials_required)
                    entry.subtitle != null -> entry.subtitle
                    else -> stringResource(if (entry.isContainer) R.string.filer_folder else R.string.filer_file)
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TextButton(onClick = onMore) { Text(stringResource(R.string.filer_more_actions)) }
    }
    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.25f))
}
