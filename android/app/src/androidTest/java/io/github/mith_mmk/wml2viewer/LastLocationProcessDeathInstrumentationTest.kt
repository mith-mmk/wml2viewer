package io.github.mith_mmk.wml2viewer

import android.content.Intent
import android.provider.DocumentsContract
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import io.github.mith_mmk.wml2viewer.data.config.MobileConfigRepository
import io.github.mith_mmk.wml2viewer.data.config.MobileLastLocation
import io.github.mith_mmk.wml2viewer.data.config.MobileLastLocationStore
import io.github.mith_mmk.wml2viewer.data.config.MobileSourceProfileStore
import io.github.mith_mmk.wml2viewer.data.config.proto.SafProfileV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import io.github.mith_mmk.wml2viewer.platform.saf.FakeSafDocumentsProvider
import io.github.mith_mmk.wml2viewer.platform.saf.SafSourceProvider
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * CI invokes this class twice with an `am force-stop` between phases. Keeping the phases in one
 * test class makes the durable seed explicit without pretending that Activity recreation is a
 * process death.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class LastLocationProcessDeathInstrumentationTest {
    @Test
    fun durableLastLocationSurvivesTheTwoPhaseProcessBoundary() = runBlocking {
        val arguments = InstrumentationRegistry.getArguments()
        val phase = arguments.getString(ARG_PHASE)
        assumeTrue("This test requires the CI two-phase process-death harness", phase in PHASES)
        val runId = requireNotNull(arguments.getString(ARG_RUN_ID)).takeIf(String::isNotBlank)
            ?: error("A nonblank process-death run ID is required")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val instrumentationContext = InstrumentationRegistry.getInstrumentation().context
        val repository = MobileConfigRepository(context)
        val store = MobileLastLocationStore(repository)
        val sourceProfiles = MobileSourceProfileStore(repository)
        val treeUri = DocumentsContract.buildTreeDocumentUri(
            FakeSafDocumentsProvider.AUTHORITY,
            FakeSafDocumentsProvider.ROOT_ID,
        )
        val accessFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        val grantFlags = accessFlags or
            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
            Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
        if (phase == PHASE_SEED) {
            val initialized = (context.applicationContext as Wml2ViewerApplication).awaitComponent()
            withTimeout(30_000) { initialized.viewerController.snapshot.first { !it.loading } }
            instrumentationContext.startActivity(
                Intent()
                    .setClassName(context.packageName, Wml2ViewerActivity::class.java.name)
                    .setData(treeUri)
                    .addFlags(grantFlags or Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            InstrumentationRegistry.getInstrumentation().waitForIdleSync()
            context.contentResolver.takePersistableUriPermission(treeUri, accessFlags)
        }
        val provider = SafSourceProvider(context, treeUri)
        val folder = provider.list(provider.root).single { it.name == "folder" }
        val archive = provider.list(folder.ref).single { it.name == "process-death.zip" }
        val expected = MobileLastLocation(
            sourceId = provider.providerId,
            directoryOpaqueEntryId = folder.ref.opaqueId,
            openedEntryOpaqueEntryId = archive.ref.opaqueId,
            logicalPageIndex = 17,
            openedArchive = true,
        )

        when (phase) {
            PHASE_SEED -> {
                repository.update { config ->
                    config.toBuilder()
                        .setFiler(config.filer.toBuilder().setRememberLastLocation(true))
                        .build()
                }
                sourceProfiles.upsert(
                    SourceProfileV1.newBuilder()
                        .setSourceId(provider.providerId)
                        .setDisplayName("Process death source $runId")
                        .setSaf(SafProfileV1.newBuilder().setTreeUri(treeUri.toString()))
                        .build(),
                )
                store.replace(expected)
                assertEquals(expected, store.currentIfRemembered())
            }
            PHASE_VERIFY -> {
                val restored = store.currentIfRemembered()
                assertNotNull("Last location disappeared across process death", restored)
                assertEquals(expected, restored)
                val application = context.applicationContext as Wml2ViewerApplication
                val component = application.awaitComponent()
                val snapshot = withTimeout(30_000) {
                    component.viewerController.snapshot.first { state ->
                        !state.loading &&
                            state.breadcrumb.lastOrNull()?.label == "folder" &&
                            state.currentLogicalPageIndex == 17
                    }
                }
                assertEquals("folder", snapshot.breadcrumb.last().label)
                assertEquals(2, snapshot.breadcrumb.size)
                assertEquals(false, snapshot.atSourceRoot)
                assertEquals(17, snapshot.currentLogicalPageIndex)
                assertEquals(FakeSafDocumentsProvider.ARCHIVE_PAGE_COUNT, snapshot.mangaPages.size)
                assertNull(snapshot.error)
                assertNotNull(snapshot.frame)
                try {
                    store.replace(null)
                    sourceProfiles.remove(provider.providerId)
                    context.contentResolver.releasePersistableUriPermission(
                        treeUri,
                        accessFlags,
                    )
                } finally {
                    instrumentationContext.revokeUriPermission(treeUri, accessFlags)
                }
            }
        }
    }

    private companion object {
        const val ARG_PHASE = "lastLocationPhase"
        const val ARG_RUN_ID = "lastLocationRunId"
        const val PHASE_SEED = "seed"
        const val PHASE_VERIFY = "verify"
        val PHASES = setOf(PHASE_SEED, PHASE_VERIFY)
    }
}
