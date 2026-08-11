package io.github.mith_mmk.wml2viewer.platform.saf

import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsProvider
import android.util.Base64
import java.io.File
import java.io.FileNotFoundException
import java.io.FileOutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** Small, deterministic DocumentsProvider used only by on-device SAF contract tests. */
class FakeSafDocumentsProvider : DocumentsProvider() {
    override fun onCreate(): Boolean = true

    override fun queryRoots(projection: Array<out String>?): Cursor = cursor(
        projection ?: DEFAULT_ROOT_PROJECTION,
        listOf(
            mapOf(
                DocumentsContract.Root.COLUMN_ROOT_ID to ROOT_ID,
                DocumentsContract.Root.COLUMN_DOCUMENT_ID to ROOT_ID,
                DocumentsContract.Root.COLUMN_TITLE to "Instrumentation root",
                DocumentsContract.Root.COLUMN_FLAGS to DocumentsContract.Root.FLAG_SUPPORTS_CREATE,
                DocumentsContract.Root.COLUMN_AVAILABLE_BYTES to 1_048_576L,
            ),
        ),
    )

    override fun queryDocument(
        documentId: String?,
        projection: Array<out String>?,
    ): Cursor {
        val id = documentId ?: throw FileNotFoundException("Missing document id")
        denyIfRevoked(id)
        val document = DOCUMENTS[id] ?: throw FileNotFoundException("Unknown document")
        return cursor(projection ?: DEFAULT_DOCUMENT_PROJECTION, listOf(document.columns))
    }

    override fun queryChildDocuments(
        parentDocumentId: String?,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val parentId = parentDocumentId ?: throw FileNotFoundException("Missing parent id")
        denyIfRevoked(parentId)
        if (parentId != ROOT_ID && parentId != FOLDER_ID) throw FileNotFoundException("Unknown parent")
        return cursor(
            projection ?: DEFAULT_DOCUMENT_PROJECTION,
            DOCUMENTS.values.filter { it.parentId == parentId }.map { it.columns },
        )
    }

    override fun openDocument(
        documentId: String?,
        mode: String?,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        if (documentId != ARCHIVE_ID || mode?.startsWith("r") != true) {
            throw FileNotFoundException("The fake provider exposes only the process-death archive")
        }
        val archive = File(requireNotNull(context).cacheDir, ".test-process-death-book.zip")
        ZipOutputStream(FileOutputStream(archive, false)).use { output ->
            repeat(ARCHIVE_PAGE_COUNT) { index ->
                output.putNextEntry(ZipEntry("page-${index.toString().padStart(2, '0')}.png"))
                output.write(TINY_PNG)
                output.closeEntry()
            }
        }
        return ParcelFileDescriptor.open(archive, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    private fun denyIfRevoked(documentId: String) {
        if (documentId == REVOKED_ROOT_ID) {
            throw SecurityException("Instrumentation provider intentionally revoked this tree")
        }
    }

    private fun cursor(
        projection: Array<out String>,
        rows: List<Map<String, Any?>>,
    ): Cursor = MatrixCursor(projection).apply {
        rows.forEach { values ->
            newRow().also { row -> projection.forEach { column -> row.add(values[column]) } }
        }
    }

    private data class FakeDocument(
        val parentId: String?,
        val columns: Map<String, Any?>,
    )

    companion object {
        const val AUTHORITY = "io.github.mith_mmk.wml2viewer.test.documents"
        const val ROOT_ID = "root"
        const val REVOKED_ROOT_ID = "revoked"
        const val FOLDER_ID = "$ROOT_ID/folder"
        const val ARCHIVE_ID = "$FOLDER_ID/process-death.zip"
        const val ARCHIVE_PAGE_COUNT = 18

        private val DEFAULT_ROOT_PROJECTION = arrayOf(
            DocumentsContract.Root.COLUMN_ROOT_ID,
            DocumentsContract.Root.COLUMN_DOCUMENT_ID,
            DocumentsContract.Root.COLUMN_TITLE,
            DocumentsContract.Root.COLUMN_FLAGS,
            DocumentsContract.Root.COLUMN_AVAILABLE_BYTES,
        )
        private val DEFAULT_DOCUMENT_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            DocumentsContract.Document.COLUMN_FLAGS,
        )
        private val DOCUMENTS = listOf(
            document(
                id = ROOT_ID,
                parentId = null,
                name = "Instrumentation root",
                mimeType = DocumentsContract.Document.MIME_TYPE_DIR,
                flags = DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE or
                    DocumentsContract.Document.FLAG_SUPPORTS_RENAME or
                    DocumentsContract.Document.FLAG_SUPPORTS_DELETE,
            ),
            document(
                id = "$ROOT_ID/editable.png",
                parentId = ROOT_ID,
                name = "editable.png",
                mimeType = "image/png",
                size = 128L,
                flags = DocumentsContract.Document.FLAG_SUPPORTS_COPY or
                    DocumentsContract.Document.FLAG_SUPPORTS_MOVE or
                    DocumentsContract.Document.FLAG_SUPPORTS_RENAME or
                    DocumentsContract.Document.FLAG_SUPPORTS_DELETE or
                    DocumentsContract.Document.FLAG_SUPPORTS_THUMBNAIL,
            ),
            document(
                id = FOLDER_ID,
                parentId = ROOT_ID,
                name = "folder",
                mimeType = DocumentsContract.Document.MIME_TYPE_DIR,
                flags = DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE,
            ),
            document(
                id = ARCHIVE_ID,
                parentId = FOLDER_ID,
                name = "process-death.zip",
                mimeType = "application/zip",
                flags = 0,
            ),
            document(
                id = "$ROOT_ID/read-only.jpg",
                parentId = ROOT_ID,
                name = "read-only.jpg",
                mimeType = "image/jpeg",
                size = 64L,
                flags = 0,
            ),
        ).associateBy { it.columns.getValue(DocumentsContract.Document.COLUMN_DOCUMENT_ID) as String }

        private val TINY_PNG = Base64.decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+Nq2VAAAAAElFTkSuQmCC",
            Base64.DEFAULT,
        )

        private fun document(
            id: String,
            parentId: String?,
            name: String,
            mimeType: String,
            size: Long? = null,
            flags: Int,
        ) = FakeDocument(
            parentId = parentId,
            columns = mapOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID to id,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME to name,
                DocumentsContract.Document.COLUMN_MIME_TYPE to mimeType,
                DocumentsContract.Document.COLUMN_SIZE to size,
                DocumentsContract.Document.COLUMN_LAST_MODIFIED to 1_700_000_000_000L,
                DocumentsContract.Document.COLUMN_FLAGS to flags,
            ),
        )
    }
}
