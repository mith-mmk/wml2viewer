package io.github.mith_mmk.wml2viewer.platform.saf

import android.Manifest
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsProvider
import androidx.test.core.app.ApplicationProvider
import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.shadows.ShadowContentResolver
import java.io.FileNotFoundException
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
class SafSafeReplacementTest {
    private lateinit var documents: ReplacementDocumentsProvider
    private lateinit var provider: SafSourceProvider

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val authority = "$AUTHORITY_PREFIX.${AUTHORITY_SEQUENCE.incrementAndGet()}"
        documents = ReplacementDocumentsProvider()
        documents.attachInfo(
            context,
            ProviderInfo().apply {
                this.authority = authority
                exported = true
                grantUriPermissions = true
                readPermission = Manifest.permission.MANAGE_DOCUMENTS
                writePermission = Manifest.permission.MANAGE_DOCUMENTS
            },
        )
        ShadowContentResolver.registerProviderInternal(authority, documents)
        provider = SafSourceProvider(
            context,
            DocumentsContract.buildTreeDocumentUri(authority, ReplacementDocumentsProvider.ROOT_ID),
        )
    }

    @Test
    fun failedReplaceCopyRestoresExistingDestination() {
        val sourceDirectory = documents.addDirectory(ReplacementDocumentsProvider.ROOT_ID, "source")
        documents.addFile(sourceDirectory, "target.bin", "new")
        documents.addFile(ReplacementDocumentsProvider.ROOT_ID, "target.bin", "old")
        documents.failCopyAfterCreation = true

        assertThrows(SourceException::class.java) {
            runTest {
                val sourceParent = child(provider.root, "source")
                provider.copy(
                    child(sourceParent, "target.bin"), provider.root, "target.bin",
                    CollisionPolicy.REPLACE,
                )
            }
        }

        assertThat(documents.contents(sourceDirectory, "target.bin")).isEqualTo("new")
        assertThat(documents.contents(ReplacementDocumentsProvider.ROOT_ID, "target.bin")).isEqualTo("old")
        assertThat(documents.hiddenNames()).isEmpty()
    }

    @Test
    fun failedReplaceMoveRestoresSourceAndExistingDestination() {
        val sourceDirectory = documents.addDirectory(ReplacementDocumentsProvider.ROOT_ID, "source")
        val destinationDirectory = documents.addDirectory(ReplacementDocumentsProvider.ROOT_ID, "destination")
        documents.addFile(sourceDirectory, "source.bin", "new")
        documents.addFile(destinationDirectory, "target.bin", "old")
        documents.failAfterNextRenameTo = "target.bin"
        documents.changeIdOnMove = true

        assertThrows(SourceException::class.java) {
            runTest {
                val sourceParent = child(provider.root, "source")
                val destinationParent = child(provider.root, "destination")
                provider.move(
                    child(sourceParent, "source.bin"), destinationParent, "target.bin",
                    CollisionPolicy.REPLACE,
                )
            }
        }

        assertThat(documents.contents(sourceDirectory, "source.bin")).isEqualTo("new")
        assertThat(documents.contents(destinationDirectory, "target.bin")).isEqualTo("old")
        assertThat(documents.hiddenNames()).isEmpty()
    }

    @Test
    fun failedReplaceRenameRestoresBothNames() {
        documents.addFile(ReplacementDocumentsProvider.ROOT_ID, "source.bin", "new")
        documents.addFile(ReplacementDocumentsProvider.ROOT_ID, "target.bin", "old")
        documents.failAfterNextRenameTo = "target.bin"

        assertThrows(SourceException::class.java) {
            runTest {
                provider.rename(
                    child(provider.root, "source.bin"), "target.bin", CollisionPolicy.REPLACE,
                )
            }
        }

        assertThat(documents.contents(ReplacementDocumentsProvider.ROOT_ID, "source.bin")).isEqualTo("new")
        assertThat(documents.contents(ReplacementDocumentsProvider.ROOT_ID, "target.bin")).isEqualTo("old")
        assertThat(documents.hiddenNames()).isEmpty()
    }

    @Test
    fun selfTargetMoveAndRenameAreNoOps() = runTest {
        documents.addFile(ReplacementDocumentsProvider.ROOT_ID, "same.bin", "original")
        val entry = child(provider.root, "same.bin")

        assertThat(provider.move(entry, provider.root, "same.bin", CollisionPolicy.REPLACE)).isEqualTo(entry)
        assertThat(provider.rename(entry, "same.bin", CollisionPolicy.REPLACE)).isEqualTo(entry)
        assertThat(documents.moveCalls).isEqualTo(0)
        assertThat(documents.renameCalls).isEqualTo(0)
        assertThat(documents.contents(ReplacementDocumentsProvider.ROOT_ID, "same.bin")).isEqualTo("original")
    }

    @Test
    fun treeRootCopyAndDeleteNeverReachDocumentsProvider() {
        assertThrows(SourceException::class.java) {
            runTest {
                provider.copy(
                    provider.root, provider.root, "root-copy", CollisionPolicy.KEEP_BOTH,
                )
            }
        }
        assertThrows(SourceException::class.java) {
            runTest { provider.trashOrDelete(provider.root, allowPermanentDelete = true) }
        }

        assertThat(documents.copyCalls).isEqualTo(0)
        assertThat(documents.deleteCalls).isEqualTo(0)
    }

    private suspend fun child(parent: EntryRef, name: String): EntryRef =
        provider.list(parent).single { it.name == name }.ref

    private companion object {
        const val AUTHORITY_PREFIX = "io.github.mith_mmk.wml2viewer.test.documents"
        val AUTHORITY_SEQUENCE = AtomicInteger()
    }
}

private class ReplacementDocumentsProvider : DocumentsProvider() {
    private data class Node(
        val id: String,
        var parentId: String?,
        var name: String,
        val mimeType: String,
        val contents: String? = null,
    )

    private val nodes = linkedMapOf(
        ROOT_ID to Node(ROOT_ID, null, "root", DocumentsContract.Document.MIME_TYPE_DIR),
    )
    private var nextId = 1
    var failCopyAfterCreation = false
    var failNextRenameTo: String? = null
    var failAfterNextRenameTo: String? = null
    var changeIdOnMove = false
    var copyCalls = 0
    var deleteCalls = 0
    var moveCalls = 0
    var renameCalls = 0

    override fun onCreate() = true

    fun addDirectory(parentId: String, name: String): String =
        addNode(parentId, name, DocumentsContract.Document.MIME_TYPE_DIR, null)

    fun addFile(parentId: String, name: String, contents: String): String =
        addNode(parentId, name, "application/octet-stream", contents)

    fun contents(parentId: String, name: String): String? =
        nodes.values.singleOrNull { it.parentId == parentId && it.name == name }?.contents

    fun hiddenNames(): List<String> = nodes.values.map { it.name }.filter { it.startsWith(".wml2viewer-") }

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val columns = projection?.map { it }?.toTypedArray() ?: ROOT_PROJECTION
        return MatrixCursor(columns).apply {
            addRow(Array<Any?>(columns.size) { index ->
                val column = columns[index]
                when (column) {
                    DocumentsContract.Root.COLUMN_ROOT_ID -> ROOT_ID
                    DocumentsContract.Root.COLUMN_DOCUMENT_ID -> ROOT_ID
                    DocumentsContract.Root.COLUMN_TITLE -> "Test"
                    DocumentsContract.Root.COLUMN_FLAGS -> DocumentsContract.Root.FLAG_SUPPORTS_CREATE
                    DocumentsContract.Root.COLUMN_MIME_TYPES -> "*/*"
                    else -> null
                }
            })
        }
    }

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        documentCursor(node(documentId), projection)

    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        node(parentDocumentId)
        val columns = projection?.map { it }?.toTypedArray() ?: DOCUMENT_PROJECTION
        return MatrixCursor(columns).apply {
            nodes.values.filter { it.parentId == parentDocumentId }.forEach { child ->
                addRow(documentRow(child, columns))
            }
        }
    }

    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor = throw FileNotFoundException("Not needed by safe replacement tests")

    override fun copyDocument(sourceDocumentId: String, targetParentDocumentId: String): String {
        copyCalls++
        val source = node(sourceDocumentId)
        node(targetParentDocumentId)
        val copiedId = addNode(targetParentDocumentId, source.name, source.mimeType, source.contents)
        if (failCopyAfterCreation) {
            failCopyAfterCreation = false
            throw FileNotFoundException("Injected post-copy failure")
        }
        return copiedId
    }

    override fun moveDocument(
        sourceDocumentId: String,
        sourceParentDocumentId: String,
        targetParentDocumentId: String,
    ): String {
        moveCalls++
        val source = node(sourceDocumentId)
        if (source.parentId != sourceParentDocumentId) throw FileNotFoundException("Wrong source parent")
        node(targetParentDocumentId)
        if (!changeIdOnMove) {
            source.parentId = targetParentDocumentId
            return source.id
        }
        val movedId = "node-${nextId++}"
        nodes.remove(source.id)
        nodes[movedId] = source.copy(id = movedId, parentId = targetParentDocumentId)
        return movedId
    }

    override fun renameDocument(documentId: String, displayName: String): String {
        renameCalls++
        if (failNextRenameTo == displayName) {
            failNextRenameTo = null
            throw FileNotFoundException("Injected rename failure")
        }
        node(documentId).name = displayName
        if (failAfterNextRenameTo == displayName) {
            failAfterNextRenameTo = null
            throw FileNotFoundException("Injected post-rename failure")
        }
        return documentId
    }

    override fun deleteDocument(documentId: String) {
        deleteCalls++
        node(documentId)
        nodes.values.filter { it.parentId == documentId }.map { it.id }.forEach(::deleteDocument)
        nodes.remove(documentId)
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean {
        var current = nodes[documentId] ?: return false
        while (current.parentId != null) {
            if (current.parentId == parentDocumentId) return true
            current = nodes[current.parentId] ?: return false
        }
        return parentDocumentId == documentId
    }

    private fun documentCursor(node: Node, projection: Array<out String>?): Cursor {
        val columns = projection?.map { it }?.toTypedArray() ?: DOCUMENT_PROJECTION
        return MatrixCursor(columns).apply { addRow(documentRow(node, columns)) }
    }

    private fun documentRow(node: Node, columns: Array<String>): Array<Any?> = columns.map { column ->
        when (column) {
            DocumentsContract.Document.COLUMN_DOCUMENT_ID -> node.id
            DocumentsContract.Document.COLUMN_DISPLAY_NAME -> node.name
            DocumentsContract.Document.COLUMN_MIME_TYPE -> node.mimeType
            DocumentsContract.Document.COLUMN_SIZE -> node.contents?.toByteArray()?.size?.toLong()
            DocumentsContract.Document.COLUMN_LAST_MODIFIED -> 1L
            DocumentsContract.Document.COLUMN_FLAGS -> if (node.mimeType == DocumentsContract.Document.MIME_TYPE_DIR) {
                DIRECTORY_FLAGS
            } else {
                FILE_FLAGS
            }
            else -> null
        }
    }.toTypedArray()

    private fun addNode(parentId: String, name: String, mimeType: String, contents: String?): String {
        node(parentId)
        val id = "node-${nextId++}"
        nodes[id] = Node(id, parentId, name, mimeType, contents)
        return id
    }

    private fun node(id: String): Node = nodes[id] ?: throw FileNotFoundException("Missing document: $id")

    companion object {
        const val ROOT_ID = "root"
        private val ROOT_PROJECTION = arrayOf(
            DocumentsContract.Root.COLUMN_ROOT_ID,
            DocumentsContract.Root.COLUMN_DOCUMENT_ID,
            DocumentsContract.Root.COLUMN_TITLE,
            DocumentsContract.Root.COLUMN_FLAGS,
            DocumentsContract.Root.COLUMN_MIME_TYPES,
        )
        private val DOCUMENT_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_FLAGS,
        )
        private const val FILE_FLAGS = DocumentsContract.Document.FLAG_SUPPORTS_COPY or
            DocumentsContract.Document.FLAG_SUPPORTS_MOVE or
            DocumentsContract.Document.FLAG_SUPPORTS_RENAME or
            DocumentsContract.Document.FLAG_SUPPORTS_DELETE
        private const val DIRECTORY_FLAGS = FILE_FLAGS or DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE
    }
}
