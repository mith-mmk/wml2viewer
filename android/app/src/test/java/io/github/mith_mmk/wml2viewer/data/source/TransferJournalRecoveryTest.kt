package io.github.mith_mmk.wml2viewer.data.transfer

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.work.ListenableWorker
import androidx.work.testing.TestListenableWorkerBuilder
import androidx.work.workDataOf
import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceProviderRegistry
import io.github.mith_mmk.wml2viewer.data.source.SourceTransferEngine
import io.github.mith_mmk.wml2viewer.data.source.TreeTestProvider
import io.github.mith_mmk.wml2viewer.data.source.TransferOperation
import io.github.mith_mmk.wml2viewer.data.source.sha256AndSize
import java.io.ByteArrayInputStream
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class TransferJournalRecoveryTest {
    private lateinit var database: TransferDatabase

    @Before fun setUp() {
        val context: Context = ApplicationProvider.getApplicationContext()
        database = Room.inMemoryDatabaseBuilder(context, TransferDatabase::class.java).allowMainThreadQueries().build()
    }
    @After fun tearDown() { database.close() }

    @Test
    fun processRecoveryPreservesCommittedReferenceAndPhase() = runTest {
        val dao = database.transferJobs()
        val job = TransferJobV1.create(
            EntryRef("source", "file"), EntryRef("destination", "root"), "copy.cbz",
            TransferOperation.MOVE, CollisionPolicy.FAIL, nowEpochMillis = 1, jobId = "job",
        )
        dao.insert(job)
        assertThat(dao.claim("job", 2)).isEqualTo(1)
        dao.markCopying("job", "destination", "temp", "copy (2).cbz", null, 3)
        dao.markVerifying("job", 5, "abc", 4)
        dao.markCommitted("job", "destination", "final", 5)
        assertThat(dao.recoverInterrupted(6)).isEqualTo(1)
        val recovered = dao.get("job")!!
        assertThat(recovered.state).isEqualTo(TransferJobState.QUEUED)
        assertThat(recovered.phase).isEqualTo(TransferJobPhase.COMMITTED)
        assertThat(recovered.committed).isEqualTo(EntryRef("destination", "final"))
        assertThat(recovered.plannedFinalName).isNull()
    }

    @Test
    fun notificationCancellationPersistsJournalFlagBeforeWorkStops() = runTest {
        val dao = database.transferJobs()
        dao.insert(TransferJobV1.create(
            EntryRef("source", "file"), EntryRef("destination", "root"), "copy.cbz",
            TransferOperation.COPY, CollisionPolicy.FAIL, nowEpochMillis = 1, jobId = "cancel-me",
        ))
        TransferCancellation.markRequested(dao, "cancel-me", 2)
        assertThat(dao.get("cancel-me")!!.cancelRequested).isTrue()
    }

    @Test
    fun workManagerInterruptionRequeuesWithoutMasqueradingAsUserCancellation() = runTest {
        val dao = database.transferJobs()
        dao.insert(
            TransferJobV1.create(
                EntryRef("source", "file"),
                EntryRef("destination", "root"),
                "copy.cbz",
                TransferOperation.COPY,
                CollisionPolicy.FAIL,
                nowEpochMillis = 1,
                jobId = "os-stop",
            ),
        )
        assertThat(dao.claim("os-stop", 2)).isEqualTo(1)
        dao.markCopying("os-stop", "destination", "temp", "copy.cbz", null, 3)

        assertThat(persistWorkerInterruption(dao, "os-stop", 4)).isFalse()

        val interrupted = dao.get("os-stop")!!
        assertThat(interrupted.state).isEqualTo(TransferJobState.QUEUED)
        assertThat(interrupted.cancelRequested).isFalse()
        assertThat(interrupted.phase).isEqualTo(TransferJobPhase.COPYING)
        assertThat(interrupted.staging).isEqualTo(EntryRef("destination", "temp"))
        assertThat(interrupted.plannedFinalName).isEqualTo("copy.cbz")
    }

    @Test
    fun persistedUserCancellationRemainsTerminalWhenWorkerIsStopped() = runTest {
        val dao = database.transferJobs()
        dao.insert(
            TransferJobV1.create(
                EntryRef("source", "file"),
                EntryRef("destination", "root"),
                "copy.cbz",
                TransferOperation.COPY,
                CollisionPolicy.FAIL,
                nowEpochMillis = 1,
                jobId = "user-stop",
            ),
        )
        assertThat(dao.claim("user-stop", 2)).isEqualTo(1)
        dao.requestCancel("user-stop", 3)

        assertThat(persistWorkerInterruption(dao, "user-stop", 4)).isTrue()
        assertThat(dao.get("user-stop")!!.state).isEqualTo(TransferJobState.CANCELLED)
    }

    @Test
    fun foregroundIsEstablishedBeforeAnyResumeWork() = runTest {
        val events = mutableListOf<String>()

        val result = foregroundBeforeTransferResume(
            establishForeground = { events += "foreground" },
            resume = {
                events += "resume"
                "done"
            },
        )

        assertThat(result).isEqualTo("done")
        assertThat(events).containsExactly("foreground", "resume").inOrder()
    }

    @Test
    fun verifyingRecoveryKeepsIdPreservingPublishedFileAndDeletesMoveSourceOnce() = runTest {
        val sourceProvider = TreeTestProvider("source")
        val destinationProvider = TreeTestProvider("destination")
        val source = sourceProvider.putFile("source.bin", byteArrayOf(1, 2, 3))
        val payload = byteArrayOf(4, 5, 6)
        val staging = destinationProvider.putFile(".wml2viewer.part", payload)
        val expected = ByteArrayInputStream(payload).sha256AndSize()
        var sourceDeletes = 0
        sourceProvider.beforeDelete = { sourceDeletes++ }

        val dao = database.transferJobs()
        dao.insert(
            TransferJobV1.create(
                source,
                destinationProvider.root,
                "page.bin",
                TransferOperation.MOVE,
                CollisionPolicy.FAIL,
                nowEpochMillis = 1,
                jobId = "id-preserving-publish",
            ),
        )
        assertThat(dao.claim("id-preserving-publish", 2)).isEqualTo(1)
        dao.markCopying(
            "id-preserving-publish",
            staging.providerId,
            staging.opaqueId,
            "page.bin",
            null,
            3,
        )
        dao.markVerifying("id-preserving-publish", expected.byteCount, expected.sha256, 4)
        val published = destinationProvider.rename(staging, "page.bin", CollisionPolicy.FAIL)
        assertThat(published).isEqualTo(staging)
        assertThat(dao.recoverInterrupted(5)).isEqualTo(1)

        TransferWorkerRuntime.install(
            TransferWorkerDependencies(
                database,
                SourceTransferEngine(SourceProviderRegistry(listOf(sourceProvider, destinationProvider))),
                clock = { 6 },
            ),
        )
        try {
            val worker = TestListenableWorkerBuilder<TransferJobWorker>(
                ApplicationProvider.getApplicationContext(),
            ).setInputData(
                workDataOf(TransferJobWorker.KEY_JOB_ID to "id-preserving-publish"),
            ).build()

            assertThat(worker.doWork()).isInstanceOf(ListenableWorker.Result.Success::class.java)
        } finally {
            TransferWorkerRuntime.clear()
        }

        assertThat(sourceProvider.exists("source.bin")).isFalse()
        assertThat(sourceDeletes).isEqualTo(1)
        assertThat(destinationProvider.bytes("page.bin")).isEqualTo(payload)
        assertThat(dao.get("id-preserving-publish")!!.state).isEqualTo(TransferJobState.SUCCEEDED)
    }

    @Test
    fun corruptPublishedReplacementSurfacesOriginalBackupWithoutMutatingVisibleFile() = runTest {
        val sourceProvider = TreeTestProvider("source")
        val destinationProvider = TreeTestProvider("destination")
        val expectedPayload = byteArrayOf(4, 5, 6)
        val source = sourceProvider.putFile("source.bin", expectedPayload)
        val originalPayload = byteArrayOf(9, 8, 7)
        val original = destinationProvider.putFile("page.bin", originalPayload)
        val backupName = ".wml2viewer-backup-corrupt-replace"
        destinationProvider.rename(original, backupName, CollisionPolicy.FAIL)
        val corruptPayload = byteArrayOf(1, 1, 1)
        val corruptFinal = destinationProvider.putFile("page.bin", corruptPayload)
        val expected = ByteArrayInputStream(expectedPayload).sha256AndSize()

        val dao = database.transferJobs()
        dao.insert(
            TransferJobV1.create(
                source,
                destinationProvider.root,
                "page.bin",
                TransferOperation.COPY,
                CollisionPolicy.REPLACE,
                nowEpochMillis = 1,
                jobId = "corrupt-replace",
            ),
        )
        assertThat(dao.claim("corrupt-replace", 2)).isEqualTo(1)
        dao.markCopying(
            "corrupt-replace",
            corruptFinal.providerId,
            corruptFinal.opaqueId,
            "page.bin",
            backupName,
            3,
        )
        dao.markVerifying("corrupt-replace", expected.byteCount, expected.sha256, 4)
        assertThat(dao.recoverInterrupted(5)).isEqualTo(1)

        TransferWorkerRuntime.install(
            TransferWorkerDependencies(
                database,
                SourceTransferEngine(SourceProviderRegistry(listOf(sourceProvider, destinationProvider))),
                clock = { 6 },
            ),
        )
        try {
            val worker = TestListenableWorkerBuilder<TransferJobWorker>(
                ApplicationProvider.getApplicationContext(),
            ).setInputData(
                workDataOf(TransferJobWorker.KEY_JOB_ID to "corrupt-replace"),
            ).build()

            assertThat(worker.doWork()).isInstanceOf(ListenableWorker.Result.Failure::class.java)
        } finally {
            TransferWorkerRuntime.clear()
        }

        assertThat(destinationProvider.bytes("page.bin")).isEqualTo(corruptPayload)
        assertThat(destinationProvider.bytes("page (recovered-corruptr).bin")).isEqualTo(originalPayload)
        assertThat(destinationProvider.exists(backupName)).isFalse()
        assertThat(sourceProvider.bytes("source.bin")).isEqualTo(expectedPayload)
        assertThat(dao.get("corrupt-replace")!!.state).isEqualTo(TransferJobState.FAILED)
    }

    @Test
    fun ambiguousReplacementSurfacesBackupWithoutChangingVisibleOrStagingFiles() = runTest {
        val sourceProvider = TreeTestProvider("source")
        val destinationProvider = TreeTestProvider("destination")
        val source = sourceProvider.putFile("source.bin", byteArrayOf(4, 5, 6))
        val originalPayload = byteArrayOf(9, 8, 7)
        val original = destinationProvider.putFile("page.bin", originalPayload)
        val backupName = ".wml2viewer-backup-ambiguous"
        destinationProvider.rename(original, backupName, CollisionPolicy.FAIL)
        val visiblePayload = byteArrayOf(1, 1, 1)
        destinationProvider.putFile("page.bin", visiblePayload)
        val stagingPayload = byteArrayOf(4, 5, 6)
        val staging = destinationProvider.putFile(".wml2viewer-part-ambiguous", stagingPayload)
        val expected = ByteArrayInputStream(stagingPayload).sha256AndSize()

        val dao = database.transferJobs()
        dao.insert(
            TransferJobV1.create(
                source,
                destinationProvider.root,
                "page.bin",
                TransferOperation.COPY,
                CollisionPolicy.REPLACE,
                nowEpochMillis = 1,
                jobId = "ambiguous-replace",
            ),
        )
        assertThat(dao.claim("ambiguous-replace", 2)).isEqualTo(1)
        dao.markCopying(
            "ambiguous-replace",
            staging.providerId,
            staging.opaqueId,
            "page.bin",
            backupName,
            3,
        )
        dao.markVerifying("ambiguous-replace", expected.byteCount, expected.sha256, 4)
        assertThat(dao.recoverInterrupted(5)).isEqualTo(1)

        TransferWorkerRuntime.install(
            TransferWorkerDependencies(
                database,
                SourceTransferEngine(SourceProviderRegistry(listOf(sourceProvider, destinationProvider))),
                clock = { 6 },
            ),
        )
        try {
            val worker = TestListenableWorkerBuilder<TransferJobWorker>(
                ApplicationProvider.getApplicationContext(),
            ).setInputData(
                workDataOf(TransferJobWorker.KEY_JOB_ID to "ambiguous-replace"),
            ).build()

            assertThat(worker.doWork()).isInstanceOf(ListenableWorker.Result.Failure::class.java)
        } finally {
            TransferWorkerRuntime.clear()
        }

        assertThat(destinationProvider.bytes("page.bin")).isEqualTo(visiblePayload)
        assertThat(destinationProvider.bytes(".wml2viewer-part-ambiguous")).isEqualTo(stagingPayload)
        assertThat(destinationProvider.bytes("page (recovered-ambiguou).bin")).isEqualTo(originalPayload)
        assertThat(destinationProvider.exists(backupName)).isFalse()
        assertThat(sourceProvider.bytes("source.bin")).isEqualTo(stagingPayload)
        assertThat(dao.get("ambiguous-replace")!!.state).isEqualTo(TransferJobState.FAILED)
    }
}
