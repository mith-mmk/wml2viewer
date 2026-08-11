package io.github.mith_mmk.wml2viewer.platform.saf

import android.content.ContentResolver
import android.content.Context
import android.graphics.Bitmap
import android.net.Uri
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.util.Base64
import android.util.Size
import io.github.mith_mmk.wml2viewer.data.source.AtomicWriteSession
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.CollisionResolution
import io.github.mith_mmk.wml2viewer.data.source.CollisionResolver
import io.github.mith_mmk.wml2viewer.data.source.CreateRequest
import io.github.mith_mmk.wml2viewer.data.source.DeleteDisposition
import io.github.mith_mmk.wml2viewer.data.source.EntryKind
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceCapabilities
import io.github.mith_mmk.wml2viewer.data.source.SourceEntry
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.data.source.SourceProvider
import io.github.mith_mmk.wml2viewer.data.source.SourceRead
import io.github.mith_mmk.wml2viewer.data.source.SourceThumbnail
import io.github.mith_mmk.wml2viewer.data.source.WriteVerification
import io.github.mith_mmk.wml2viewer.data.source.sha256AndSize
import io.github.mith_mmk.wml2viewer.data.source.toHex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.FileNotFoundException
import java.io.IOException
import java.io.OutputStream
import java.security.MessageDigest
import java.util.UUID

/** A direct DocumentsContract-backed provider. It never imports trees into app storage. */
class SafSourceProvider(
    context: Context,
    private val treeUri: Uri,
) : SourceProvider {
    private val resolver = (context.applicationContext ?: context).contentResolver
    private val treeDocumentId = DocumentsContract.getTreeDocumentId(treeUri)

    override val providerId: String = "saf:${stableId(treeUri.toString())}"
    override val root: EntryRef = ref(treeDocumentId, null)
    override val capabilities = SourceCapabilities(
        canList = true,
        canRead = true,
        canCreate = true,
        canCopyWithinProvider = true,
        canMoveWithinProvider = true,
        canRename = true,
        canTrash = false,
        canDelete = true,
        canThumbnail = true,
        hasAtomicFinalize = true,
        canCopyDirectoriesWithinProvider = true,
        canMoveDirectoriesWithinProvider = true,
        canTransferDirectoriesAcrossProviders = true,
    )

    init {
        require(treeUri.scheme == ContentResolver.SCHEME_CONTENT) { "SAF tree URI must use content://" }
        require(DocumentsContract.isTreeUri(treeUri)) { "A persisted tree URI is required" }
    }

    override suspend fun list(parent: EntryRef): List<SourceEntry> = io {
        val parentId = decode(parent).documentId
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_FLAGS,
        )
        resolver.query(childrenUri, projection, Bundle.EMPTY, null)?.use { cursor ->
            val idIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
            val modifiedIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            val flagsIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_FLAGS)
            buildList {
                while (cursor.moveToNext()) {
                    val id = cursor.getString(idIndex)
                    val mime = cursor.getString(mimeIndex)
                    val name = cursor.getString(nameIndex).orEmpty().ifBlank { "document-${stableId(id).take(8)}" }
                    val flags = cursor.getLong(flagsIndex)
                    val kind = if (mime == DocumentsContract.Document.MIME_TYPE_DIR) EntryKind.DIRECTORY else EntryKind.FILE
                    add(
                        SourceEntry(
                            ref = ref(id, parentId),
                            parent = parent,
                            name = name,
                            kind = kind,
                            mimeType = mime.takeUnless { it == DocumentsContract.Document.MIME_TYPE_DIR },
                            size = cursor.nullableLong(sizeIndex),
                            modifiedAtEpochMillis = cursor.nullableLong(modifiedIndex),
                            isHidden = name.startsWith('.'),
                            effectiveCapabilities = capabilitiesFromFlags(kind, flags),
                            platformFlags = flags,
                        ),
                    )
                }
            }.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
        } ?: throw SourceException(SourceErrorCode.IO, "The document provider returned no listing")
    }.mapSafErrors()

    override suspend fun stat(entry: EntryRef): SourceEntry = io {
        val id = decode(entry)
        queryEntry(documentUri(id.documentId), entry, id.parentDocumentId?.let { ref(it, null) })
    }.mapSafErrors()

    override suspend fun openRead(entry: EntryRef): SourceRead = io {
        val metadata = stat(entry)
        requireCapability(metadata.effectiveCapabilities.canRead, "read")
        if (metadata.kind != EntryKind.FILE) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Cannot open a directory for reading")
        }
        val descriptor = resolver.openFileDescriptor(documentUri(decode(entry).documentId), "r")
            ?: throw SourceException(SourceErrorCode.NOT_FOUND, "Document is unavailable")
        SourceRead(ParcelFileDescriptor.AutoCloseInputStream(descriptor), metadata.size, metadata.mimeType)
    }.mapSafErrors()

    override suspend fun create(parent: EntryRef, request: CreateRequest): AtomicWriteSession = io {
        if (request.kind != EntryKind.FILE) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Use createDirectory for directories")
        }
        val parentId = decode(parent).documentId
        requireCapability(stat(parent).effectiveCapabilities.canCreate, "create")
        val existing = list(parent).associateBy { it.name }
        when (val resolution = CollisionResolver.resolve(request.name, request.collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> SafSkippedWriteSession(existing.getValue(request.name).ref, request.name)
            is CollisionResolution.Use -> {
                val replacement = if (resolution.replaceExisting) existing[request.name]?.ref else null
                val replacementBackupName = replacement?.let { allocateBackupName(parent) }
                val tempName = ".wml2viewer-${UUID.randomUUID()}.part"
                val tempUri = DocumentsContract.createDocument(
                    resolver,
                    documentUri(parentId),
                    request.mimeType,
                    tempName,
                ) ?: throw SourceException(SourceErrorCode.IO, "Document provider could not create a temporary file")
                val tempId = DocumentsContract.getDocumentId(tempUri)
                val output = resolver.openFileDescriptor(tempUri, "w")?.let { descriptor ->
                    ParcelFileDescriptor.AutoCloseOutputStream(descriptor)
                }
                    ?: run {
                        DocumentsContract.deleteDocument(resolver, tempUri)
                        throw SourceException(SourceErrorCode.IO, "Document provider could not open a temporary file")
                    }
                SafAtomicWriteSession(
                    provider = this@SafSourceProvider,
                    tempRef = ref(tempId, parentId),
                    finalName = resolution.name,
                    replace = replacement,
                    replacementBackupName = replacementBackupName,
                    outputStream = output,
                )
            }
        }
    }.mapSafErrors()

    override suspend fun createDirectory(
        parent: EntryRef,
        name: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef = io {
        val parentId = decode(parent).documentId
        requireCapability(stat(parent).effectiveCapabilities.canCreate, "create directory")
        val children = list(parent).associateBy { it.name }
        when (val resolution = CollisionResolver.resolve(name, collisionPolicy) { it in children }) {
            CollisionResolution.Skip -> children.getValue(name).ref
            is CollisionResolution.Use -> {
                val replaced = if (resolution.replaceExisting) children[name]?.ref else null
                var backup: EntryRef? = null
                try {
                    if (replaced != null) {
                        backup = renameWithoutCollision(replaced, allocateBackupName(parent))
                    }
                    val created = DocumentsContract.createDocument(
                        resolver,
                        documentUri(parentId),
                        DocumentsContract.Document.MIME_TYPE_DIR,
                        resolution.name,
                    ) ?: throw SourceException(SourceErrorCode.IO, "Document provider could not create a directory")
                    val result = ref(DocumentsContract.getDocumentId(created), parentId)
                    if (backup != null) runCatching { permanentlyDelete(backup) }
                    result
                } catch (error: Throwable) {
                    if (backup != null) runCatching { renameWithoutCollision(backup, name) }
                    throw error
                }
            }
        }
    }.mapSafErrors()

    override suspend fun copy(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef = io {
        requireNotTreeRoot(source, "copy")
        val sourceInfo = stat(source)
        requireCapability(sourceInfo.effectiveCapabilities.canCopyWithinProvider, "copy")
        requireCapability(stat(destinationParent).effectiveCapabilities.canCreate, "create")
        val destination = prepareDestination(destinationParent, destinationName, collisionPolicy)
            ?: return@io findByName(destinationParent, destinationName)!!.ref
        if (destination.replaced == source) return@io source
        val backup = backupReplacement(destination)
        val sourceUri = documentUri(decode(source).documentId)
        val parentId = decode(destinationParent).documentId
        var copiedRef: EntryRef? = null
        try {
            val copiedUri = DocumentsContract.copyDocument(resolver, sourceUri, documentUri(parentId))
                ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "Document provider does not support direct copy")
            var result = ref(DocumentsContract.getDocumentId(copiedUri), parentId)
            copiedRef = result
            if (sourceInfo.name != destination.name) {
                result = renameWithoutCollision(result, destination.name)
                copiedRef = result
            }
            completeReplacement(backup)
            result
        } catch (error: Throwable) {
            deleteRollbackCopy(
                copiedRef = copiedRef,
                destinationParent = destinationParent,
                destinationName = destination.name,
                source = source,
                backup = backup,
                originalError = error,
            )
            restoreReplacement(backup, error)
            throw error
        }
    }.mapSafErrors()

    override suspend fun move(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef = io {
        val sourceId = decode(source)
        val sourceParentId = sourceId.parentDocumentId
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SAF root cannot be moved")
        val sourceInfo = stat(source)
        requireCapability(sourceInfo.effectiveCapabilities.canMoveWithinProvider, "move")
        requireCapability(stat(destinationParent).effectiveCapabilities.canCreate, "create")
        val destinationParentId = decode(destinationParent).documentId
        if (sourceParentId == destinationParentId && sourceInfo.name == destinationName) return@io source
        val destination = prepareDestination(destinationParent, destinationName, collisionPolicy)
            ?: return@io findByName(destinationParent, destinationName)!!.ref
        val backup = backupReplacement(destination)
        var movedRef = source
        try {
            if (sourceParentId != destinationParentId) {
                val movedUri = DocumentsContract.moveDocument(
                    resolver,
                    documentUri(sourceId.documentId),
                    documentUri(sourceParentId),
                    documentUri(destinationParentId),
                ) ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "Document provider does not support direct move")
                movedRef = ref(DocumentsContract.getDocumentId(movedUri), destinationParentId)
            }
            if (sourceInfo.name != destination.name) {
                movedRef = renameWithoutCollision(movedRef, destination.name)
            }
            completeReplacement(backup)
            movedRef
        } catch (error: Throwable) {
            if (sourceParentId == destinationParentId) {
                restoreRenamedSource(
                    entry = source,
                    parent = destinationParent,
                    failedName = destination.name,
                    originalName = sourceInfo.name,
                    originalError = error,
                )
            } else {
                restoreMovedSource(
                    decode(movedRef).documentId,
                    destinationParentId,
                    sourceParentId,
                    destination.name,
                    sourceInfo.name,
                    error,
                )
            }
            restoreReplacement(backup, error)
            throw error
        }
    }.mapSafErrors()

    override suspend fun rename(
        entry: EntryRef,
        newName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef = io {
        val id = decode(entry)
        val sourceInfo = stat(entry)
        requireCapability(sourceInfo.effectiveCapabilities.canRename, "rename")
        val parentId = id.parentDocumentId
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SAF root cannot be renamed")
        if (sourceInfo.name == newName) return@io entry
        val parent = ref(parentId, null)
        val destination = prepareDestination(parent, newName, collisionPolicy)
            ?: return@io findByName(parent, newName)!!.ref
        val backup = backupReplacement(destination)
        try {
            val renamed = renameWithoutCollision(entry, destination.name)
            completeReplacement(backup)
            renamed
        } catch (error: Throwable) {
            restoreRenamedSource(
                entry = entry,
                parent = parent,
                failedName = destination.name,
                originalName = sourceInfo.name,
                originalError = error,
            )
            restoreReplacement(backup, error)
            throw error
        }
    }.mapSafErrors()

    override suspend fun trashOrDelete(
        entry: EntryRef,
        allowPermanentDelete: Boolean,
    ): DeleteDisposition = io {
        requireNotTreeRoot(entry, "delete")
        if (!allowPermanentDelete) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "This document provider has no portable trash API")
        }
        requireCapability(stat(entry).effectiveCapabilities.canDelete, "delete")
        permanentlyDelete(entry)
        DeleteDisposition.PERMANENTLY_DELETED
    }.mapSafErrors()

    override suspend fun thumbnail(entry: EntryRef, maxWidth: Int, maxHeight: Int): SourceThumbnail? = io {
        require(maxWidth > 0 && maxHeight > 0) { "Thumbnail dimensions must be positive" }
        val metadata = stat(entry)
        if (metadata.kind != EntryKind.FILE) return@io null
        requireCapability(metadata.effectiveCapabilities.canThumbnail, "thumbnail")
        val bitmap = resolver.loadThumbnail(
            documentUri(decode(entry).documentId),
            Size(maxWidth, maxHeight),
            CancellationSignal(),
        )
        try {
            val bytes = ByteArrayOutputStream()
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, bytes)) { "Unable to encode thumbnail" }
            SourceThumbnail(bytes.toByteArray(), "image/png", bitmap.width, bitmap.height)
        } finally {
            bitmap.recycle()
        }
    }.mapSafErrors()

    internal suspend fun verifyTemporary(ref: EntryRef, expected: WriteVerification): Boolean = io {
        resolver.openInputStream(documentUri(decode(ref).documentId))?.use { input ->
            input.sha256AndSize() == expected
        } ?: false
    }

    internal suspend fun commitTemporary(
        temp: EntryRef,
        finalName: String,
        replace: EntryRef?,
        replacementBackupName: String?,
    ): EntryRef = io {
        var backup: EntryRef? = null
        try {
            if (replace != null) {
                val parentId = decode(replace).parentDocumentId
                    ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SAF tree root cannot be replaced")
                val backupName = replacementBackupName
                    ?: throw SourceException(SourceErrorCode.INTEGRITY, "Replacement backup plan is missing")
                backup = renameWithoutCollision(replace, backupName)
            }
            val committed = renameWithoutCollision(temp, finalName)
            if (backup != null) runCatching { permanentlyDelete(backup) }
            committed
        } catch (error: Throwable) {
            if (backup != null) runCatching { renameWithoutCollision(backup, finalName) }
            throw error
        }
    }

    internal suspend fun abortTemporary(temp: EntryRef) = io {
        runCatching { permanentlyDelete(temp) }
        Unit
    }

    private suspend fun prepareDestination(
        parent: EntryRef,
        requestedName: String,
        policy: CollisionPolicy,
    ): PreparedDestination? {
        val children = list(parent)
        val byName = children.associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(requestedName, policy) { it in byName }) {
            CollisionResolution.Skip -> null
            is CollisionResolution.Use -> PreparedDestination(
                resolution.name,
                if (resolution.replaceExisting) byName[requestedName]?.ref else null,
            )
        }
    }

    private suspend fun findByName(parent: EntryRef, name: String): SourceEntry? = list(parent).firstOrNull { it.name == name }

    private suspend fun backupReplacement(destination: PreparedDestination): ReplacementBackup? {
        val replaced = destination.replaced ?: return null
        val parentId = decode(replaced).parentDocumentId
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "The SAF tree root cannot be replaced")
        val backup = renameWithoutCollision(replaced, allocateBackupName(ref(parentId, null)))
        return ReplacementBackup(backup, destination.name)
    }

    private suspend fun allocateBackupName(parent: EntryRef): String {
        repeat(MAX_BACKUP_NAME_ATTEMPTS) {
            val candidate = ".wml2viewer-backup-${UUID.randomUUID()}"
            if (findByName(parent, candidate) == null) return candidate
        }
        throw SourceException(SourceErrorCode.ALREADY_EXISTS, "Unable to allocate a replacement backup")
    }

    private fun completeReplacement(backup: ReplacementBackup?) {
        if (backup != null) runCatching { permanentlyDelete(backup.ref) }
    }

    private fun restoreReplacement(backup: ReplacementBackup?, originalError: Throwable) {
        if (backup == null) return
        try {
            renameWithoutCollision(backup.ref, backup.originalName)
        } catch (rollbackError: Throwable) {
            originalError.addSuppressed(rollbackError)
        }
    }

    private suspend fun deleteRollbackCopy(
        copiedRef: EntryRef?,
        destinationParent: EntryRef,
        destinationName: String,
        source: EntryRef,
        backup: ReplacementBackup?,
        originalError: Throwable,
    ) {
        var directDeleteError: Throwable? = null
        if (copiedRef != null) {
            try {
                permanentlyDelete(copiedRef)
                return
            } catch (error: Throwable) {
                directDeleteError = error
            }
        }
        try {
            val published = findByName(destinationParent, destinationName)?.ref
            if (published != null && published != source && published != backup?.ref) {
                permanentlyDelete(published)
                return
            }
        } catch (rollbackError: Throwable) {
            originalError.addSuppressed(rollbackError)
        }
        directDeleteError?.let(originalError::addSuppressed)
    }

    private suspend fun restoreRenamedSource(
        entry: EntryRef,
        parent: EntryRef,
        failedName: String,
        originalName: String,
        originalError: Throwable,
    ) {
        var directRestoreError: Throwable? = null
        try {
            renameWithoutCollision(entry, originalName)
            return
        } catch (rollbackError: Throwable) {
            directRestoreError = rollbackError
        }
        try {
            if (findByName(parent, originalName) != null) return
            val renamedSource = findByName(parent, failedName)?.ref
                ?: throw SourceException(SourceErrorCode.NOT_FOUND, "Renamed source could not be recovered")
            renameWithoutCollision(renamedSource, originalName)
        } catch (fallbackError: Throwable) {
            directRestoreError?.let(originalError::addSuppressed)
            originalError.addSuppressed(fallbackError)
        }
    }

    private suspend fun restoreMovedSource(
        documentId: String,
        currentParentId: String,
        originalParentId: String,
        failedName: String,
        originalName: String,
        originalError: Throwable,
    ) {
        val currentParent = ref(currentParentId, null)
        val originalParent = ref(originalParentId, null)
        try {
            val restored = try {
                moveDocumentForRollback(documentId, currentParentId, originalParentId)
            } catch (directError: Throwable) {
                val alreadyRestored = findByName(originalParent, originalName)
                    ?: findByName(originalParent, failedName)
                if (alreadyRestored != null) {
                    alreadyRestored.ref
                } else {
                    val movedSource = findByName(currentParent, failedName)
                        ?: findByName(currentParent, originalName)
                        ?: throw directError
                    moveDocumentForRollback(
                        decode(movedSource.ref).documentId,
                        currentParentId,
                        originalParentId,
                    )
                }
            }
            val restoredId = decode(restored).documentId
            val restoredInfo = queryEntry(documentUri(restoredId), restored, ref(originalParentId, null))
            if (restoredInfo.name != originalName) renameWithoutCollision(restored, originalName)
        } catch (rollbackError: Throwable) {
            originalError.addSuppressed(rollbackError)
        }
    }

    private fun moveDocumentForRollback(
        documentId: String,
        currentParentId: String,
        originalParentId: String,
    ): EntryRef {
        val restoredUri = DocumentsContract.moveDocument(
            resolver,
            documentUri(documentId),
            documentUri(currentParentId),
            documentUri(originalParentId),
        ) ?: throw SourceException(SourceErrorCode.IO, "Document provider could not restore the source")
        return ref(DocumentsContract.getDocumentId(restoredUri), originalParentId)
    }

    private fun renameWithoutCollision(entry: EntryRef, name: String): EntryRef {
        CollisionResolver.validateFileName(name)
        val id = decode(entry)
        val renamed = DocumentsContract.renameDocument(resolver, documentUri(id.documentId), name)
            ?: throw SourceException(SourceErrorCode.UNSUPPORTED, "Document provider does not support rename")
        return ref(DocumentsContract.getDocumentId(renamed), id.parentDocumentId)
    }

    private fun permanentlyDelete(entry: EntryRef) {
        if (!DocumentsContract.deleteDocument(resolver, documentUri(decode(entry).documentId))) {
            throw SourceException(SourceErrorCode.IO, "Document provider could not delete the entry")
        }
    }

    private fun queryEntry(uri: Uri, ref: EntryRef, parent: EntryRef?): SourceEntry {
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_FLAGS,
        )
        resolver.query(uri, projection, Bundle.EMPTY, null)?.use { cursor ->
            if (!cursor.moveToFirst()) throw SourceException(SourceErrorCode.NOT_FOUND, "Document not found")
            val name = cursor.getString(0).orEmpty().ifBlank { "document-${stableId(uri.toString()).take(8)}" }
            val mime = cursor.getString(1)
            val flags = cursor.getLong(4)
            val kind = if (mime == DocumentsContract.Document.MIME_TYPE_DIR) EntryKind.DIRECTORY else EntryKind.FILE
            return SourceEntry(
                ref,
                parent,
                name,
                kind,
                mime.takeUnless { it == DocumentsContract.Document.MIME_TYPE_DIR },
                cursor.nullableLong(2),
                cursor.nullableLong(3),
                name.startsWith('.'),
                capabilitiesFromFlags(kind, flags),
                flags,
            )
        }
        throw SourceException(SourceErrorCode.NOT_FOUND, "Document not found")
    }

    private fun documentUri(documentId: String): Uri =
        DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)

    private fun ref(documentId: String, parentDocumentId: String?): EntryRef =
        EntryRef(providerId, SafOpaqueIdCodec.encode(documentId, parentDocumentId))

    private fun decode(ref: EntryRef): SafOpaqueId {
        if (ref.providerId != providerId) {
            throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Entry belongs to another provider")
        }
        return try {
            SafOpaqueIdCodec.decode(ref.opaqueId)
        } catch (error: IllegalArgumentException) {
            throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Malformed SAF entry reference", error)
        }
    }

    private fun requireFile(entry: SourceEntry) {
        if (entry.kind != EntryKind.FILE) throw SourceException(SourceErrorCode.UNSUPPORTED, "Directory copy is not supported")
    }

    private fun requireCapability(allowed: Boolean, operation: String) {
        if (!allowed) throw SourceException(SourceErrorCode.UNSUPPORTED, "Document provider does not support $operation")
    }

    private fun requireNotTreeRoot(entry: EntryRef, operation: String) {
        if (decode(entry).documentId == treeDocumentId) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "The SAF tree root cannot be used for $operation")
        }
    }

    private fun capabilitiesFromFlags(kind: EntryKind, flags: Long): SourceCapabilities = SourceCapabilities(
        canList = kind == EntryKind.DIRECTORY,
        canRead = kind == EntryKind.FILE,
        canCreate = kind == EntryKind.DIRECTORY && flags.hasFlag(DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE),
        canCopyWithinProvider = flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_COPY),
        canMoveWithinProvider = flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_MOVE),
        canRename = flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_RENAME),
        canTrash = false,
        canDelete = flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_DELETE),
        canThumbnail = kind == EntryKind.FILE && flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_THUMBNAIL),
        hasAtomicFinalize = flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_RENAME),
        canCopyDirectoriesWithinProvider = kind == EntryKind.DIRECTORY &&
            flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_COPY),
        canMoveDirectoriesWithinProvider = kind == EntryKind.DIRECTORY &&
            flags.hasFlag(DocumentsContract.Document.FLAG_SUPPORTS_MOVE),
        canTransferDirectoriesAcrossProviders = kind == EntryKind.DIRECTORY,
    )

    private suspend fun <T> io(block: suspend () -> T): T = withContext(Dispatchers.IO) {
        try {
            block()
        } catch (error: SourceException) {
            throw error
        } catch (error: SecurityException) {
            throw SourceException(SourceErrorCode.ACCESS_DENIED, "Document provider denied access", error)
        } catch (error: FileNotFoundException) {
            throw SourceException(SourceErrorCode.NOT_FOUND, "Document is unavailable", error)
        } catch (error: UnsupportedOperationException) {
            throw SourceException(SourceErrorCode.UNSUPPORTED, "Document provider operation is unsupported", error)
        } catch (error: IOException) {
            throw SourceException(SourceErrorCode.IO, "Document provider I/O failed", error)
        }
    }

    private suspend fun <T> T.mapSafErrors(): T = this

    private data class PreparedDestination(val name: String, val replaced: EntryRef?)
    private data class ReplacementBackup(val ref: EntryRef, val originalName: String)

    companion object {
        private const val MAX_BACKUP_NAME_ATTEMPTS = 16
        private fun stableId(value: String): String =
            MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8)).toHex()
    }
}

private data class SafOpaqueId(val documentId: String, val parentDocumentId: String?)

private object SafOpaqueIdCodec {
    private const val PREFIX = "v1"

    fun encode(documentId: String, parentDocumentId: String?): String =
        "$PREFIX.${encodePart(documentId)}.${encodePart(parentDocumentId.orEmpty())}"

    fun decode(value: String): SafOpaqueId {
        val parts = value.split('.', limit = 3)
        require(parts.size == 3 && parts[0] == PREFIX) { "Unsupported SAF reference" }
        val documentId = decodePart(parts[1])
        require(documentId.isNotEmpty()) { "Missing document id" }
        return SafOpaqueId(documentId, decodePart(parts[2]).ifEmpty { null })
    }

    private fun encodePart(value: String): String =
        Base64.encodeToString(value.toByteArray(Charsets.UTF_8), Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)

    private fun decodePart(value: String): String =
        String(Base64.decode(value, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING), Charsets.UTF_8)
}

private class SafSkippedWriteSession(
    private val existing: EntryRef,
    override val plannedFinalName: String,
) : AtomicWriteSession {
    override val temporaryRef = existing
    override val replacementBackupName: String? = null
    override val skippedExistingRef = existing
    override val output: OutputStream = object : OutputStream() {
        override fun write(value: Int) = Unit
    }
    override suspend fun verify(expected: WriteVerification) = true
    override suspend fun commit() = existing
    override suspend fun abort() = Unit
}

private class SafAtomicWriteSession(
    private val provider: SafSourceProvider,
    tempRef: EntryRef,
    private val finalName: String,
    private val replace: EntryRef?,
    override val replacementBackupName: String?,
    outputStream: OutputStream,
) : AtomicWriteSession {
    override val temporaryRef = tempRef
    override val plannedFinalName = finalName
    override val output = outputStream
    private var verified = false
    private var finished = false

    override suspend fun verify(expected: WriteVerification): Boolean {
        check(!finished) { "Write session is finished" }
        runCatching { output.close() }
        return provider.verifyTemporary(temporaryRef, expected).also { verified = it }
    }

    override suspend fun commit(): EntryRef {
        check(!finished) { "Write session is finished" }
        check(verified) { "Write must be verified before commit" }
        return provider.commitTemporary(
            temporaryRef,
            finalName,
            replace,
            replacementBackupName,
        ).also { finished = true }
    }

    override suspend fun abort() {
        if (finished) return
        finished = true
        runCatching { output.close() }
        provider.abortTemporary(temporaryRef)
    }
}

private fun android.database.Cursor.nullableLong(index: Int): Long? =
    if (isNull(index)) null else getLong(index).takeIf { it >= 0L }

private fun Long.hasFlag(flag: Int): Boolean = this and flag.toLong() != 0L
