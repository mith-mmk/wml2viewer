package io.github.mith_mmk.wml2viewer.data.source

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import java.io.FilterInputStream
import java.io.InputStream
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.CancellationException

enum class TransferOperation { COPY, MOVE }

data class TransferProgress(val bytesCopied: Long, val totalBytes: Long?)

data class TransferResult(
    val destination: EntryRef,
    val byteCount: Long,
    val sha256: String,
    val sourceDeleted: Boolean,
    val skipped: Boolean = false,
)

interface TransferJournal {
    suspend fun copying(
        temporary: EntryRef,
        plannedFinalName: String,
        replacementBackupName: String?,
    )
    suspend fun verifying(verification: WriteVerification)
    suspend fun committed(destination: EntryRef)
    suspend fun deletingSource()

    data object None : TransferJournal {
        override suspend fun copying(
            temporary: EntryRef,
            plannedFinalName: String,
            replacementBackupName: String?,
        ) = Unit
        override suspend fun verifying(verification: WriteVerification) = Unit
        override suspend fun committed(destination: EntryRef) = Unit
        override suspend fun deletingSource() = Unit
    }
}

class SourceTransferEngine(
    private val registry: SourceProviderRegistry,
    private val bufferSize: Int = DEFAULT_BUFFER_SIZE,
) {
    init {
        require(bufferSize in 4 * 1024..4 * 1024 * 1024) { "Invalid transfer buffer size" }
    }

    suspend fun transfer(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
        operation: TransferOperation,
        isCancelled: suspend () -> Boolean = { false },
        onProgress: suspend (TransferProgress) -> Unit = {},
        journal: TransferJournal = TransferJournal.None,
    ): TransferResult {
        val sourceProvider = registry.require(source)
        val destinationProvider = registry.require(destinationParent)
        val sourceStat = sourceProvider.stat(source)
        val destinationStat = destinationProvider.stat(destinationParent)
        if (destinationStat.kind != EntryKind.DIRECTORY || !destinationStat.effectiveCapabilities.canCreate) {
            throw SourceException(SourceErrorCode.ACCESS_DENIED, "Destination directory is read-only")
        }
        if (sourceStat.parent == destinationParent && sourceStat.name == destinationName) {
            val noOp = when (operation) {
                TransferOperation.MOVE -> true
                TransferOperation.COPY -> when (collisionPolicy) {
                    CollisionPolicy.REPLACE, CollisionPolicy.SKIP -> true
                    CollisionPolicy.FAIL -> throw SourceException(
                        SourceErrorCode.ALREADY_EXISTS,
                        "Source and destination are the same entry",
                    )
                    CollisionPolicy.KEEP_BOTH -> false
                }
            }
            if (noOp) {
                journal.committed(source)
                return TransferResult(
                    destination = source,
                    byteCount = sourceStat.size ?: 0L,
                    sha256 = "",
                    sourceDeleted = false,
                    skipped = true,
                )
            }
        }
        if (sourceStat.kind != EntryKind.FILE) {
            if (source.providerId == destinationParent.providerId) {
                ensureDestinationOutsideSource(sourceProvider, sourceStat, destinationParent)
            }
            return transferDirectoryAcrossProviders(
                sourceProvider = sourceProvider,
                destinationProvider = destinationProvider,
                source = sourceStat,
                destinationParent = destinationParent,
                destinationName = destinationName,
                collisionPolicy = collisionPolicy,
                operation = operation,
                isCancelled = isCancelled,
                onProgress = onProgress,
                journal = journal,
            )
        }
        if (!sourceStat.effectiveCapabilities.canRead) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Source file cannot be read")
        }
        if (operation == TransferOperation.MOVE && !sourceStat.effectiveCapabilities.canDelete) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Source file cannot be deleted after transfer")
        }

        val pending = destinationProvider.create(
            destinationParent,
            CreateRequest(destinationName, sourceStat.mimeType ?: "application/octet-stream", collisionPolicy),
        )
        pending.skippedExistingRef?.let { existing ->
            pending.abort()
            return TransferResult(existing, 0, "", sourceDeleted = false, skipped = true)
        }
        var committed: EntryRef? = null
        try {
            journal.copying(
                pending.temporaryRef,
                pending.plannedFinalName,
                pending.replacementBackupName,
            )
            val digest = MessageDigest.getInstance("SHA-256")
            var copied = 0L
            sourceProvider.openRead(source).use { read ->
                read.stream.use { input ->
                    pending.output.use { output ->
                        val buffer = ByteArray(bufferSize)
                        while (true) {
                            if (isCancelled()) throw ExplicitTransferCancellation()
                            val count = input.read(buffer)
                            if (count < 0) break
                            output.write(buffer, 0, count)
                            digest.update(buffer, 0, count)
                            copied += count
                            onProgress(TransferProgress(copied, sourceStat.size))
                        }
                        output.flush()
                    }
                }
            }
            val expected = WriteVerification(copied, digest.digest().toHex())
            journal.verifying(expected)
            if (!pending.verify(expected)) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Destination verification failed")
            }
            committed = pending.commit()
            journal.committed(committed)
            val deleted = if (operation == TransferOperation.MOVE) {
                journal.deletingSource()
                sourceProvider.trashOrDelete(source, allowPermanentDelete = true)
                true
            } else {
                false
            }
            return TransferResult(committed, copied, expected.sha256, deleted)
        } catch (cancelled: ExplicitTransferCancellation) {
            withContext(NonCancellable) { pending.abort() }
            throw SourceException(SourceErrorCode.CANCELLED, "Transfer cancelled", cancelled)
        } catch (cancelled: CancellationException) {
            withContext(NonCancellable) { pending.abort() }
            throw cancelled
        } catch (error: Throwable) {
            if (committed == null) withContext(NonCancellable) { pending.abort() }
            if (error is SourceException) throw error
            throw SourceException(SourceErrorCode.IO, "Transfer failed", error)
        }
    }

    suspend fun cleanupStaging(ref: EntryRef) {
        withContext(NonCancellable) {
            runCatching { deleteTree(registry.require(ref), ref) }
        }
    }

    suspend fun deleteCommittedMoveSource(source: EntryRef) {
        withContext(NonCancellable) { deleteTree(registry.require(source), source) }
    }

    suspend fun findVerifiedDestination(
        destinationParent: EntryRef,
        plannedFinalName: String,
        expected: WriteVerification,
    ): EntryRef? {
        val provider = registry.require(destinationParent)
        val candidate = provider.list(destinationParent).singleOrNull { it.name == plannedFinalName }
            ?: return null
        val actual = when (candidate.kind) {
            EntryKind.FILE -> provider.openRead(candidate.ref).use { it.stream.sha256AndSize() }
            EntryKind.DIRECTORY -> calculateTreeVerification(provider, candidate.ref)
        }
        return candidate.ref.takeIf { actual == expected }
    }

    suspend fun entryExists(ref: EntryRef): Boolean = try {
        registry.require(ref).stat(ref)
        true
    } catch (error: SourceException) {
        if (error.code == SourceErrorCode.NOT_FOUND) false else throw error
    }

    suspend fun statOrNull(ref: EntryRef): SourceEntry? = try {
        registry.require(ref).stat(ref)
    } catch (error: SourceException) {
        if (error.code == SourceErrorCode.NOT_FOUND) null else throw error
    }

    suspend fun findEntryByName(parent: EntryRef, name: String): SourceEntry? =
        registry.require(parent).list(parent).singleOrNull { it.name == name }

    suspend fun verifyEntry(ref: EntryRef, expected: WriteVerification): Boolean {
        val provider = registry.require(ref)
        val entry = provider.stat(ref)
        val actual = when (entry.kind) {
            EntryKind.FILE -> provider.openRead(ref).use { it.stream.sha256AndSize() }
            EntryKind.DIRECTORY -> calculateTreeVerification(provider, ref)
        }
        return actual == expected
    }

    suspend fun restoreReplacementBackup(
        backup: EntryRef,
        plannedFinalName: String,
    ): EntryRef = registry.require(backup).rename(
        backup,
        plannedFinalName,
        CollisionPolicy.FAIL,
    )

    /**
     * Makes an app-created replacement backup visible without mutating an
     * unverified destination that may contain user data.
     */
    suspend fun surfaceReplacementBackup(
        backup: EntryRef,
        plannedFinalName: String,
        recoveryToken: String,
    ): EntryRef = registry.require(backup).rename(
        backup,
        recoveredBackupName(plannedFinalName, recoveryToken),
        CollisionPolicy.KEEP_BOTH,
    )

    private suspend fun transferDirectoryAcrossProviders(
        sourceProvider: SourceProvider,
        destinationProvider: SourceProvider,
        source: SourceEntry,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
        operation: TransferOperation,
        isCancelled: suspend () -> Boolean,
        onProgress: suspend (TransferProgress) -> Unit,
        journal: TransferJournal,
    ): TransferResult {
        if (
            !source.effectiveCapabilities.canTransferDirectoriesAcrossProviders ||
            !destinationProvider.capabilities.canTransferDirectoriesAcrossProviders ||
            !destinationProvider.capabilities.canMoveDirectoriesWithinProvider
        ) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Provider cannot transfer directories across sources")
        }
        if (operation == TransferOperation.MOVE && !source.effectiveCapabilities.canDelete) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Source directory cannot be deleted after transfer")
        }
        val destinationResolution = resolveDirectoryDestination(
            destinationProvider,
            destinationParent,
            destinationName,
            collisionPolicy,
        )
        if (destinationResolution is DirectoryDestinationResolution.Skip) {
            return TransferResult(
                destinationResolution.existing,
                byteCount = 0,
                sha256 = "",
                sourceDeleted = false,
                skipped = true,
            )
        }
        destinationResolution as DirectoryDestinationResolution.Use
        val tracker = CreatedDestinationTracker(destinationProvider)
        var committed: EntryRef? = null
        var replacementBackup: EntryRef? = null
        try {
            val plan = buildDirectoryPlan(
                provider = sourceProvider,
                root = source,
                requireDelete = operation == TransferOperation.MOVE,
                isCancelled = isCancelled,
            )
            ensureNotCancelled(isCancelled)
            val staging = destinationProvider.createDirectory(
                destinationParent,
                ".wml2viewer-tree-${UUID.randomUUID()}.part",
                CollisionPolicy.FAIL,
            )
            tracker.track(staging)
            journal.copying(
                staging,
                destinationResolution.name,
                destinationResolution.replacementBackupName,
            )

            val progress = DirectoryCopyProgress(plan.totalBytes)
            copyDirectoryChildren(
                sourceProvider,
                destinationProvider,
                plan.children,
                staging,
                tracker,
                progress,
                isCancelled,
                onProgress,
            )
            ensureNotCancelled(isCancelled)

            val destinationVerification = calculateTreeVerification(destinationProvider, staging, isCancelled)
            val sourceVerification = calculateTreeVerification(sourceProvider, source.ref, isCancelled)
            if (destinationVerification != sourceVerification) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Directory tree verification failed")
            }
            journal.verifying(destinationVerification)
            ensureNotCancelled(isCancelled)

            withContext(NonCancellable) {
                replacementBackup = destinationResolution.replaced?.let { replaced ->
                    destinationProvider.move(
                        replaced,
                        destinationParent,
                        checkNotNull(destinationResolution.replacementBackupName),
                        CollisionPolicy.FAIL,
                    )
                }
                committed = destinationProvider.move(
                    staging,
                    destinationParent,
                    destinationResolution.name,
                    CollisionPolicy.FAIL,
                )
                tracker.finalized()
                replacementBackup?.let { deleteTree(destinationProvider, it) }
                replacementBackup = null
                journal.committed(checkNotNull(committed))
                if (operation == TransferOperation.MOVE) {
                    journal.deletingSource()
                    deleteTree(sourceProvider, source.ref)
                }
            }
            return TransferResult(
                destination = checkNotNull(committed),
                byteCount = destinationVerification.byteCount,
                sha256 = destinationVerification.sha256,
                sourceDeleted = operation == TransferOperation.MOVE,
            )
        } catch (cancelled: ExplicitTransferCancellation) {
            val error = SourceException(SourceErrorCode.CANCELLED, "Directory transfer cancelled", cancelled)
            if (committed == null) {
                val safeToRollbackStaging = restoreDirectoryReplacement(
                    destinationProvider,
                    destinationParent,
                    destinationResolution.name,
                    replacementBackup,
                    destinationResolution.replacementBackupName,
                    destinationResolution.replaced,
                    error,
                )
                if (safeToRollbackStaging) attachRollbackFailures(error, tracker) else tracker.finalized()
            }
            throw error
        } catch (cancelled: CancellationException) {
            if (committed == null) {
                val safeToRollbackStaging = restoreDirectoryReplacement(
                    destinationProvider,
                    destinationParent,
                    destinationResolution.name,
                    replacementBackup,
                    destinationResolution.replacementBackupName,
                    destinationResolution.replaced,
                    cancelled,
                )
                if (safeToRollbackStaging) attachRollbackFailures(cancelled, tracker) else tracker.finalized()
            }
            throw cancelled
        } catch (error: Throwable) {
            if (committed == null) {
                val safeToRollbackStaging = restoreDirectoryReplacement(
                    destinationProvider,
                    destinationParent,
                    destinationResolution.name,
                    replacementBackup,
                    destinationResolution.replacementBackupName,
                    destinationResolution.replaced,
                    error,
                )
                if (safeToRollbackStaging) attachRollbackFailures(error, tracker) else tracker.finalized()
            }
            if (error is SourceException) throw error
            throw SourceException(SourceErrorCode.IO, "Directory transfer failed", error)
        }
    }

    private suspend fun resolveDirectoryDestination(
        provider: SourceProvider,
        parent: EntryRef,
        requestedName: String,
        collisionPolicy: CollisionPolicy,
    ): DirectoryDestinationResolution {
        val existing = provider.list(parent).associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(requestedName, collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> DirectoryDestinationResolution.Skip(existing.getValue(requestedName).ref)
            is CollisionResolution.Use -> {
                val replaced = existing[requestedName].takeIf { resolution.replaceExisting }
                if (replaced != null &&
                    (!replaced.effectiveCapabilities.canMoveDirectoriesWithinProvider ||
                        !replaced.effectiveCapabilities.canDelete)
                ) {
                    throw SourceException(
                        SourceErrorCode.UNSUPPORTED,
                        "Destination directory cannot be safely replaced",
                    )
                }
                DirectoryDestinationResolution.Use(
                    name = resolution.name,
                    replaced = replaced?.ref,
                    replacementBackupName = replaced?.let {
                        ".wml2viewer-backup-${UUID.randomUUID()}"
                    },
                )
            }
        }
    }

    private suspend fun restoreDirectoryReplacement(
        provider: SourceProvider,
        destinationParent: EntryRef,
        plannedFinalName: String,
        backup: EntryRef?,
        backupName: String?,
        originalReplaced: EntryRef?,
        originalError: Throwable,
    ): Boolean = withContext(NonCancellable) {
        try {
            val visibleFinal = provider.list(destinationParent).singleOrNull { it.name == plannedFinalName }
            if (visibleFinal != null) {
                return@withContext visibleFinal.ref == originalReplaced
            }
            val recoverableBackup = backup ?: backupName?.let { expected ->
                provider.list(destinationParent).singleOrNull { it.name == expected }?.ref
            }
            if (recoverableBackup != null) {
                provider.move(recoverableBackup, destinationParent, plannedFinalName, CollisionPolicy.FAIL)
            }
            true
        } catch (restoreError: Throwable) {
            originalError.addSuppressed(restoreError)
            false
        }
    }

    private suspend fun ensureDestinationOutsideSource(
        provider: SourceProvider,
        source: SourceEntry,
        destinationParent: EntryRef,
    ) {
        var current: EntryRef? = destinationParent
        repeat(MAX_DIRECTORY_DEPTH + 1) {
            val ref = current ?: return
            if (ref == source.ref) {
                throw SourceException(SourceErrorCode.INVALID_REFERENCE, "A directory cannot contain its own copy")
            }
            current = provider.stat(ref).parent
        }
        if (current != null) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Destination ancestry is too deep")
        }
    }

    private suspend fun buildDirectoryPlan(
        provider: SourceProvider,
        root: SourceEntry,
        requireDelete: Boolean,
        isCancelled: suspend () -> Boolean,
    ): PlannedDirectory {
        val visited = HashSet<EntryRef>()
        var nodeCount = 0

        suspend fun walk(entry: SourceEntry, depth: Int): PlannedNode {
            ensureNotCancelled(isCancelled)
            if (depth > MAX_DIRECTORY_DEPTH) {
                throw SourceException(SourceErrorCode.UNSUPPORTED, "Directory nesting is too deep")
            }
            if (!visited.add(entry.ref)) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Directory cycle detected")
            }
            if (requireDelete && !entry.effectiveCapabilities.canDelete) {
                throw SourceException(SourceErrorCode.UNSUPPORTED, "Source tree contains an entry that cannot be deleted")
            }
            nodeCount++
            if (nodeCount > MAX_DIRECTORY_NODES) {
                throw SourceException(SourceErrorCode.UNSUPPORTED, "Directory contains too many entries")
            }
            return when (entry.kind) {
                EntryKind.FILE -> PlannedFile(entry)
                EntryKind.DIRECTORY -> {
                    val children = provider.list(entry.ref)
                        .sortedWith(compareBy<SourceEntry>({ it.name }, { it.ref.opaqueId }))
                        .map { child ->
                            CollisionResolver.validateFileName(child.name)
                            walk(child, depth + 1)
                        }
                    PlannedDirectory(entry, children)
                }
            }
        }

        return walk(root, 0) as? PlannedDirectory
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "Source is not a directory")
    }

    private suspend fun copyDirectoryChildren(
        sourceProvider: SourceProvider,
        destinationProvider: SourceProvider,
        children: List<PlannedNode>,
        destinationParent: EntryRef,
        tracker: CreatedDestinationTracker,
        progress: DirectoryCopyProgress,
        isCancelled: suspend () -> Boolean,
        onProgress: suspend (TransferProgress) -> Unit,
    ) {
        for (child in children) {
            ensureNotCancelled(isCancelled)
            when (child) {
                is PlannedDirectory -> {
                    val created = destinationProvider.createDirectory(
                        destinationParent,
                        child.source.name,
                        CollisionPolicy.FAIL,
                    )
                    tracker.track(created)
                    copyDirectoryChildren(
                        sourceProvider,
                        destinationProvider,
                        child.children,
                        created,
                        tracker,
                        progress,
                        isCancelled,
                        onProgress,
                    )
                }
                is PlannedFile -> copyDirectoryFile(
                    sourceProvider,
                    destinationProvider,
                    child.source,
                    destinationParent,
                    tracker,
                    progress,
                    isCancelled,
                    onProgress,
                )
            }
        }
    }

    private suspend fun copyDirectoryFile(
        sourceProvider: SourceProvider,
        destinationProvider: SourceProvider,
        source: SourceEntry,
        destinationParent: EntryRef,
        tracker: CreatedDestinationTracker,
        progress: DirectoryCopyProgress,
        isCancelled: suspend () -> Boolean,
        onProgress: suspend (TransferProgress) -> Unit,
    ) {
        val pending = destinationProvider.create(
            destinationParent,
            CreateRequest(
                source.name,
                source.mimeType ?: "application/octet-stream",
                CollisionPolicy.FAIL,
            ),
        )
        if (pending.skippedExistingRef != null) {
            withContext(NonCancellable) { pending.abort() }
            throw SourceException(SourceErrorCode.INTEGRITY, "Unexpected skipped directory child")
        }
        tracker.track(pending.temporaryRef)
        var fileCommitted = false
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            var copied = 0L
            sourceProvider.openRead(source.ref).use { read ->
                read.stream.use { input ->
                    pending.output.use { output ->
                        val buffer = ByteArray(bufferSize)
                        while (true) {
                            ensureNotCancelled(isCancelled)
                            val count = input.read(buffer)
                            if (count < 0) break
                            output.write(buffer, 0, count)
                            digest.update(buffer, 0, count)
                            copied = safeAdd(copied, count.toLong())
                            progress.copied = safeAdd(progress.copied, count.toLong())
                            onProgress(TransferProgress(progress.copied, progress.totalBytes))
                        }
                        output.flush()
                    }
                }
            }
            val expected = WriteVerification(copied, digest.digest().toHex())
            if (!pending.verify(expected)) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Directory child staging verification failed")
            }
            val destination = pending.commit()
            fileCommitted = true
            tracker.track(destination)
            val finalized = destinationProvider.openRead(destination).use { it.stream.sha256AndSize() }
            if (finalized != expected) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Directory child finalize verification failed")
            }
        } catch (error: Throwable) {
            if (!fileCommitted) withContext(NonCancellable) { pending.abort() }
            throw error
        }
    }

    private suspend fun calculateTreeVerification(
        provider: SourceProvider,
        root: EntryRef,
        isCancelled: suspend () -> Boolean = { false },
    ): WriteVerification {
        val digest = MessageDigest.getInstance("SHA-256")
        val visited = HashSet<EntryRef>()
        var totalBytes = 0L
        var nodeCount = 0

        suspend fun walk(entry: SourceEntry, relativePath: String, depth: Int) {
            ensureNotCancelled(isCancelled)
            if (depth > MAX_DIRECTORY_DEPTH || ++nodeCount > MAX_DIRECTORY_NODES) {
                throw SourceException(SourceErrorCode.UNSUPPORTED, "Directory verification limit exceeded")
            }
            if (!visited.add(entry.ref)) {
                throw SourceException(SourceErrorCode.INTEGRITY, "Directory cycle detected during verification")
            }
            when (entry.kind) {
                EntryKind.DIRECTORY -> {
                    digest.update('D'.code.toByte())
                    digest.updateLengthPrefixed(relativePath)
                    provider.list(entry.ref)
                        .sortedWith(compareBy<SourceEntry>({ it.name }, { it.ref.opaqueId }))
                        .forEach { child ->
                            CollisionResolver.validateFileName(child.name)
                            val childPath = if (relativePath.isEmpty()) child.name else "$relativePath/${child.name}"
                            walk(child, childPath, depth + 1)
                        }
                }
                EntryKind.FILE -> {
                    val verification = provider.openRead(entry.ref).use { it.stream.sha256AndSize() }
                    totalBytes = safeAdd(totalBytes, verification.byteCount)
                    digest.update('F'.code.toByte())
                    digest.updateLengthPrefixed(relativePath)
                    digest.updateLong(verification.byteCount)
                    digest.updateLengthPrefixed(verification.sha256)
                }
            }
        }

        val rootEntry = provider.stat(root)
        if (rootEntry.kind != EntryKind.DIRECTORY) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Tree verification requires a directory")
        }
        walk(rootEntry, "", 0)
        return WriteVerification(totalBytes, digest.digest().toHex())
    }

    private suspend fun deleteTree(
        provider: SourceProvider,
        root: EntryRef,
        visited: MutableSet<EntryRef> = HashSet(),
        depth: Int = 0,
    ) {
        if (depth > MAX_DIRECTORY_DEPTH || !visited.add(root)) {
            throw SourceException(SourceErrorCode.INTEGRITY, "Invalid directory tree during deletion")
        }
        val entry = try {
            provider.stat(root)
        } catch (error: SourceException) {
            if (error.code == SourceErrorCode.NOT_FOUND) return
            throw error
        }
        if (entry.kind == EntryKind.DIRECTORY) {
            provider.list(root).forEach { child -> deleteTree(provider, child.ref, visited, depth + 1) }
        }
        try {
            provider.trashOrDelete(root, allowPermanentDelete = true)
        } catch (error: SourceException) {
            if (error.code != SourceErrorCode.NOT_FOUND) throw error
        }
    }

    private suspend fun attachRollbackFailures(error: Throwable, tracker: CreatedDestinationTracker) {
        withContext(NonCancellable) {
            tracker.rollback().forEach(error::addSuppressed)
        }
    }

    private suspend fun ensureNotCancelled(isCancelled: suspend () -> Boolean) {
        if (isCancelled()) throw ExplicitTransferCancellation()
    }

    private fun safeAdd(left: Long, right: Long): Long = try {
        Math.addExact(left, right)
    } catch (error: ArithmeticException) {
        throw SourceException(SourceErrorCode.INTEGRITY, "Directory byte count overflow", error)
    }

    companion object {
        const val DEFAULT_BUFFER_SIZE = 256 * 1024
        private const val MAX_DIRECTORY_DEPTH = 256
        private const val MAX_DIRECTORY_NODES = 100_000
    }
}

internal fun recoveredBackupName(plannedFinalName: String, recoveryToken: String): String {
    val safeToken = recoveryToken.filter(Char::isLetterOrDigit).take(8).ifEmpty { "unknown" }
    val extensionIndex = plannedFinalName.lastIndexOf('.').takeIf { it > 0 }
    val suffix = " (recovered-$safeToken)"
    val descriptive = if (extensionIndex == null) {
        plannedFinalName + suffix
    } else {
        plannedFinalName.substring(0, extensionIndex) + suffix + plannedFinalName.substring(extensionIndex)
    }
    if (descriptive.toByteArray(Charsets.UTF_8).size <= MAX_RECOVERY_FILE_NAME_UTF8_BYTES) {
        return descriptive
    }
    val shortExtension = extensionIndex
        ?.let { plannedFinalName.substring(it).take(MAX_RECOVERY_EXTENSION_CHARS) }
        .orEmpty()
    return "recovered-$safeToken$shortExtension"
}

private const val MAX_RECOVERY_FILE_NAME_UTF8_BYTES = 120
private const val MAX_RECOVERY_EXTENSION_CHARS = 16

private class ExplicitTransferCancellation : CancellationException("Transfer cancelled")

private sealed interface DirectoryDestinationResolution {
    data class Skip(val existing: EntryRef) : DirectoryDestinationResolution
    data class Use(
        val name: String,
        val replaced: EntryRef?,
        val replacementBackupName: String?,
    ) : DirectoryDestinationResolution
}

private sealed interface PlannedNode {
    val source: SourceEntry
    val totalBytes: Long?
}

private data class PlannedFile(override val source: SourceEntry) : PlannedNode {
    override val totalBytes: Long? = source.size
}

private data class PlannedDirectory(
    override val source: SourceEntry,
    val children: List<PlannedNode>,
) : PlannedNode {
    override val totalBytes: Long? = run {
        var total = 0L
        children.forEach { child ->
            val childBytes = child.totalBytes ?: return@run null
            total = try {
                Math.addExact(total, childBytes)
            } catch (_: ArithmeticException) {
                return@run null
            }
        }
        total
    }
}

private data class DirectoryCopyProgress(
    val totalBytes: Long?,
    var copied: Long = 0,
)

private class CreatedDestinationTracker(
    private val provider: SourceProvider,
) {
    private val created = LinkedHashSet<EntryRef>()
    private var isFinalized = false

    fun track(ref: EntryRef) {
        check(!isFinalized) { "Destination tracker is finalized" }
        created += ref
    }

    fun finalized() {
        isFinalized = true
        created.clear()
    }

    suspend fun rollback(): List<Throwable> {
        if (isFinalized) return emptyList()
        val failures = ArrayList<Throwable>()
        repeat(2) {
            created.toList().asReversed().forEach { ref ->
                try {
                    provider.trashOrDelete(ref, allowPermanentDelete = true)
                    created.remove(ref)
                } catch (error: SourceException) {
                    if (error.code == SourceErrorCode.NOT_FOUND) created.remove(ref) else failures += error
                } catch (error: Throwable) {
                    failures += error
                }
            }
        }
        return failures
    }
}

private fun MessageDigest.updateLengthPrefixed(value: String) {
    val bytes = value.toByteArray(Charsets.UTF_8)
    updateLong(bytes.size.toLong())
    update(bytes)
}

private fun MessageDigest.updateLong(value: Long) {
    for (shift in 56 downTo 0 step 8) update((value ushr shift).toByte())
}

private fun String.isKeepBothVariantOf(requestedName: String): Boolean {
    val dot = requestedName.lastIndexOf('.')
    val hasExtension = dot > 0 && dot < requestedName.lastIndex
    val stem = if (hasExtension) requestedName.substring(0, dot) else requestedName
    val extension = if (hasExtension) requestedName.substring(dot) else ""
    if (!startsWith("$stem (") || !endsWith(")$extension")) return false
    val end = length - extension.length - 1
    val suffix = substring(stem.length + 2, end).toIntOrNull() ?: return false
    return suffix in 2..10_000
}

internal fun InputStream.sha256AndSize(): WriteVerification {
    val digest = MessageDigest.getInstance("SHA-256")
    var size = 0L
    DigestingInputStream(this, digest) { size += it }.use { input ->
        val buffer = ByteArray(256 * 1024)
        while (input.read(buffer) >= 0) Unit
    }
    return WriteVerification(size, digest.digest().toHex())
}

private class DigestingInputStream(
    input: InputStream,
    private val digest: MessageDigest,
    private val consumed: (Long) -> Unit,
) : FilterInputStream(input) {
    override fun read(): Int = super.read().also { value ->
        if (value >= 0) {
            digest.update(value.toByte())
            consumed(1)
        }
    }

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
        super.read(buffer, offset, length).also { count ->
            if (count > 0) {
                digest.update(buffer, offset, count)
                consumed(count.toLong())
            }
        }
}

internal fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }
