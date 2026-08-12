package io.github.mith_mmk.wml2viewer.platform.saf

import android.provider.DocumentsContract
import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SafSourceProviderInstrumentationTest {
    // Use the test APK context so its private fake provider is visible under Android 11+
    // package-visibility rules without changing the production manifest.
    private val context = InstrumentationRegistry.getInstrumentation().context

    private fun grantTree(tree: android.net.Uri) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        context.grantUriPermission(context.packageName, tree, flags)
    }

    @Test
    fun liveDocumentFlagsBecomeEntrySpecificCapabilities() = runBlocking {
        val tree = DocumentsContract.buildTreeDocumentUri(
            FakeSafDocumentsProvider.AUTHORITY,
            FakeSafDocumentsProvider.ROOT_ID,
        )
        grantTree(tree)
        val provider = SafSourceProvider(context, tree)

        val root = provider.stat(provider.root)
        assertTrue(root.effectiveCapabilities.canCreate)

        val children = provider.list(provider.root).associateBy { it.name }
        assertEquals(setOf("editable.png", "folder", "read-only.jpg"), children.keys)

        val editable = children.getValue("editable.png").effectiveCapabilities
        assertTrue(editable.canRead)
        assertTrue(editable.canCopyWithinProvider)
        assertTrue(editable.canMoveWithinProvider)
        assertTrue(editable.canRename)
        assertTrue(editable.canDelete)
        assertTrue(editable.canThumbnail)

        val readOnly = children.getValue("read-only.jpg").effectiveCapabilities
        assertTrue(readOnly.canRead)
        assertFalse(readOnly.canCopyWithinProvider)
        assertFalse(readOnly.canMoveWithinProvider)
        assertFalse(readOnly.canRename)
        assertFalse(readOnly.canDelete)

        val directory = children.getValue("folder").effectiveCapabilities
        assertTrue(directory.canList)
        assertTrue(directory.canCreate)
        assertTrue(directory.canTransferDirectoriesAcrossProviders)
    }

    @Test
    fun revokedTreePermissionIsReportedWithoutLeakingProviderDetails() = runBlocking {
        val revokedTree = DocumentsContract.buildTreeDocumentUri(
            FakeSafDocumentsProvider.AUTHORITY,
            FakeSafDocumentsProvider.REVOKED_ROOT_ID,
        )
        grantTree(revokedTree)
        val provider = SafSourceProvider(context, revokedTree)

        val error = runCatching { provider.stat(provider.root) }.exceptionOrNull()
        assertTrue(error is SourceException)
        assertEquals(SourceErrorCode.ACCESS_DENIED, (error as SourceException).code)
        assertFalse(error.message.orEmpty().contains("Instrumentation provider intentionally"))
    }
}
