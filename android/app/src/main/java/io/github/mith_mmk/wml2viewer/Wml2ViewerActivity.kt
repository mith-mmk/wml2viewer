package io.github.mith_mmk.wml2viewer

import android.app.NativeActivity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.widget.Toast
import java.io.File
import java.io.FileOutputStream

class Wml2ViewerActivity : NativeActivity() {
    private val handler = Handler(Looper.getMainLooper())
    private val pickerRequestCode = 4107
    private val pollRequest = object : Runnable {
        override fun run() {
            if (File(filesDir, "picker.request").delete()) openFolderPicker()
            handler.postDelayed(this, 250)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handler.post(pollRequest)
        val imported = File(filesDir, "imported")
        if (!imported.exists() || imported.listFiles().isNullOrEmpty()) {
            handler.post { openFolderPicker() }
        }
    }

    override fun onDestroy() {
        handler.removeCallbacks(pollRequest)
        super.onDestroy()
    }

    @Deprecated("NativeActivity uses the platform activity result callback")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != pickerRequestCode || resultCode != RESULT_OK) return
        val resultData = data ?: return
        val uri = resultData.data ?: return
        val flags = resultData.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION
        try {
            contentResolver.takePersistableUriPermission(uri, flags)
        } catch (_: SecurityException) {
            showMessage("The selected provider cannot persist access")
        }
        getSharedPreferences("storage", MODE_PRIVATE).edit().putString("tree_uri", uri.toString()).apply()
        importTreeAsync(uri)
    }

    private fun openFolderPicker() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            getSharedPreferences("storage", MODE_PRIVATE).getString("tree_uri", null)?.let {
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, Uri.parse(it))
            }
        }
        startActivityForResult(intent, pickerRequestCode)
    }

    private fun importTreeAsync(treeUri: Uri) {
        showMessage("Importing folder…")
        Thread {
            val staging = File(filesDir, ".importing")
            val destination = File(filesDir, "imported")
            try {
                staging.deleteRecursively()
                staging.mkdirs()
                copyDocumentChildren(treeUri, DocumentsContract.getTreeDocumentId(treeUri), staging)
                destination.deleteRecursively()
                if (!staging.renameTo(destination)) {
                    staging.copyRecursively(destination, overwrite = true)
                    staging.deleteRecursively()
                }
                File(filesDir, "import.ready").writeText(treeUri.toString())
                showMessage("Folder imported")
            } catch (error: Exception) {
                staging.deleteRecursively()
                showMessage("Import failed: ${error.message ?: error.javaClass.simpleName}")
            }
        }.start()
    }

    private fun copyDocumentChildren(treeUri: Uri, parentId: String, destination: File) {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            OpenableColumns.DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )
        contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameIndex = cursor.getColumnIndexOrThrow(OpenableColumns.DISPLAY_NAME)
            val mimeIndex = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            while (cursor.moveToNext()) {
                val documentId = cursor.getString(idIndex)
                val displayName = safeName(cursor.getString(nameIndex), documentId)
                val mime = cursor.getString(mimeIndex)
                if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                    val child = uniqueDestination(destination, displayName)
                    child.mkdirs()
                    copyDocumentChildren(treeUri, documentId, child)
                    if (child.listFiles().isNullOrEmpty()) child.delete()
                } else if (isSupported(displayName)) {
                    val documentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
                    contentResolver.openInputStream(documentUri)?.use { input ->
                        FileOutputStream(uniqueDestination(destination, displayName)).use { input.copyTo(it) }
                    }
                }
            }
        }
    }

    private fun safeName(name: String?, documentId: String): String {
        val cleaned = name.orEmpty().replace(Regex("[\\\\/:*?\"<>|]"), "_").trim()
        return cleaned.ifEmpty { "document_${documentId.hashCode().toUInt()}" }
    }

    private fun uniqueDestination(parent: File, name: String): File {
        var candidate = File(parent, name)
        if (!candidate.exists()) return candidate
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val extension = if (dot > 0) name.substring(dot) else ""
        var suffix = 2
        while (candidate.exists()) candidate = File(parent, "${stem}_${suffix++}$extension")
        return candidate
    }

    private fun isSupported(name: String): Boolean {
        return name.substringAfterLast('.', "").lowercase() in setOf(
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif",
            "mag", "maki", "pi", "pic", "zip", "lha", "lzh", "wmltxt"
        )
    }

    private fun showMessage(message: String) {
        runOnUiThread { Toast.makeText(this, message, Toast.LENGTH_LONG).show() }
    }
}
