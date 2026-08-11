package io.github.mith_mmk.wml2viewer.platform.saf;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract;
import android.util.Base64;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

/**
 * Deterministic DocumentsProvider used only by on-device tests.
 *
 * <p>The provider runs in the standalone test APK process, whose class path does not include the
 * target application's Kotlin runtime. Keep this implementation Java-only so provider startup is
 * independent from target APK dependencies.</p>
 */
public final class FakeSafDocumentsProvider extends ContentProvider {
    public static final String AUTHORITY = "io.github.mith_mmk.wml2viewer.test.documents";
    public static final String ROOT_ID = "root";
    public static final String REVOKED_ROOT_ID = "revoked";
    public static final String FOLDER_ID = ROOT_ID + "/folder";
    public static final String ARCHIVE_ID = FOLDER_ID + "/process-death.zip";
    public static final int ARCHIVE_PAGE_COUNT = 18;

    private static final String[] DEFAULT_DOCUMENT_PROJECTION = new String[] {
        DocumentsContract.Document.COLUMN_DOCUMENT_ID,
        DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        DocumentsContract.Document.COLUMN_MIME_TYPE,
        DocumentsContract.Document.COLUMN_SIZE,
        DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        DocumentsContract.Document.COLUMN_FLAGS,
    };
    private static final byte[] TINY_PNG = Base64.decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+Nq2VAAAAAElFTkSuQmCC",
        Base64.DEFAULT
    );
    private static final Map<String, FakeDocument> DOCUMENTS = createDocuments();

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public Cursor query(
        Uri uri,
        String[] projection,
        String selection,
        String[] selectionArgs,
        String sortOrder
    ) {
        String documentId = DocumentsContract.getDocumentId(uri);
        denyIfRevoked(documentId);
        String[] columns = projection != null ? projection : DEFAULT_DOCUMENT_PROJECTION;
        List<Map<String, Object>> rows = new ArrayList<>();
        if (uri.getPathSegments().contains("children")) {
            if (!ROOT_ID.equals(documentId) && !FOLDER_ID.equals(documentId)) {
                throw new IllegalArgumentException("Unknown parent");
            }
            for (FakeDocument document : DOCUMENTS.values()) {
                if (documentId.equals(document.parentId)) rows.add(document.columns);
            }
        } else {
            FakeDocument document = DOCUMENTS.get(documentId);
            if (document == null) throw new IllegalArgumentException("Unknown document");
            rows.add(document.columns);
        }
        return cursor(columns, rows);
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        String documentId = DocumentsContract.getDocumentId(uri);
        if (!ARCHIVE_ID.equals(documentId) || mode == null || !mode.startsWith("r")) {
            throw new FileNotFoundException(
                "The fake provider exposes only the process-death archive"
            );
        }
        File archive = new File(requireContextCacheDir(), ".test-process-death-book.zip");
        try (ZipOutputStream output = new ZipOutputStream(new FileOutputStream(archive, false))) {
            for (int index = 0; index < ARCHIVE_PAGE_COUNT; index++) {
                output.putNextEntry(new ZipEntry(String.format("page-%02d.png", index)));
                output.write(TINY_PNG);
                output.closeEntry();
            }
        } catch (IOException error) {
            FileNotFoundException wrapped = new FileNotFoundException("Unable to create archive");
            wrapped.initCause(error);
            throw wrapped;
        }
        return ParcelFileDescriptor.open(archive, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        FakeDocument document = DOCUMENTS.get(DocumentsContract.getDocumentId(uri));
        Object mime = document != null
            ? document.columns.get(DocumentsContract.Document.COLUMN_MIME_TYPE)
            : null;
        return mime instanceof String ? (String) mime : null;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException("The fake provider is read-only");
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("The fake provider is read-only");
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException("The fake provider is read-only");
    }

    private File requireContextCacheDir() throws FileNotFoundException {
        if (getContext() == null) throw new FileNotFoundException("Provider context is unavailable");
        return getContext().getCacheDir();
    }

    private static void denyIfRevoked(String documentId) {
        if (REVOKED_ROOT_ID.equals(documentId)) {
            throw new SecurityException("Instrumentation provider intentionally revoked this tree");
        }
    }

    private static Cursor cursor(String[] projection, List<Map<String, Object>> rows) {
        MatrixCursor cursor = new MatrixCursor(projection);
        for (Map<String, Object> values : rows) {
            MatrixCursor.RowBuilder row = cursor.newRow();
            for (String column : projection) row.add(values.get(column));
        }
        return cursor;
    }

    private static Map<String, FakeDocument> createDocuments() {
        Map<String, FakeDocument> documents = new LinkedHashMap<>();
        addDocument(
            documents,
            ROOT_ID,
            null,
            "Instrumentation root",
            DocumentsContract.Document.MIME_TYPE_DIR,
            null,
            DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE
                | DocumentsContract.Document.FLAG_SUPPORTS_RENAME
                | DocumentsContract.Document.FLAG_SUPPORTS_DELETE
        );
        addDocument(
            documents,
            ROOT_ID + "/editable.png",
            ROOT_ID,
            "editable.png",
            "image/png",
            128L,
            DocumentsContract.Document.FLAG_SUPPORTS_COPY
                | DocumentsContract.Document.FLAG_SUPPORTS_MOVE
                | DocumentsContract.Document.FLAG_SUPPORTS_RENAME
                | DocumentsContract.Document.FLAG_SUPPORTS_DELETE
                | DocumentsContract.Document.FLAG_SUPPORTS_THUMBNAIL
        );
        addDocument(
            documents,
            FOLDER_ID,
            ROOT_ID,
            "folder",
            DocumentsContract.Document.MIME_TYPE_DIR,
            null,
            DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE
        );
        addDocument(
            documents,
            ARCHIVE_ID,
            FOLDER_ID,
            "process-death.zip",
            "application/zip",
            null,
            0
        );
        addDocument(
            documents,
            ROOT_ID + "/read-only.jpg",
            ROOT_ID,
            "read-only.jpg",
            "image/jpeg",
            64L,
            0
        );
        return documents;
    }

    private static void addDocument(
        Map<String, FakeDocument> documents,
        String id,
        String parentId,
        String name,
        String mimeType,
        Long size,
        int flags
    ) {
        Map<String, Object> columns = new LinkedHashMap<>();
        columns.put(DocumentsContract.Document.COLUMN_DOCUMENT_ID, id);
        columns.put(DocumentsContract.Document.COLUMN_DISPLAY_NAME, name);
        columns.put(DocumentsContract.Document.COLUMN_MIME_TYPE, mimeType);
        columns.put(DocumentsContract.Document.COLUMN_SIZE, size);
        columns.put(DocumentsContract.Document.COLUMN_LAST_MODIFIED, 1_700_000_000_000L);
        columns.put(DocumentsContract.Document.COLUMN_FLAGS, flags);
        documents.put(id, new FakeDocument(parentId, columns));
    }

    private static final class FakeDocument {
        final String parentId;
        final Map<String, Object> columns;

        FakeDocument(String parentId, Map<String, Object> columns) {
            this.parentId = parentId;
            this.columns = columns;
        }
    }
}
