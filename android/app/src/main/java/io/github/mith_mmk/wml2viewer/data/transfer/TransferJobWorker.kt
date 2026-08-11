package io.github.mith_mmk.wml2viewer.data.transfer

import android.app.PendingIntent
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.data.source.SourceTransferEngine
import io.github.mith_mmk.wml2viewer.data.source.TransferJournal
import io.github.mith_mmk.wml2viewer.data.source.TransferOperation
import io.github.mith_mmk.wml2viewer.data.source.WriteVerification
import io.github.mith_mmk.wml2viewer.platform.security.SecretRedactor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.concurrent.atomic.AtomicReference

data class TransferWorkerDependencies(
    val database: TransferDatabase,
    val transferEngine: SourceTransferEngine,
    val clock: () -> Long = System::currentTimeMillis,
)

/** Must be installed by Application after providers and persisted profiles are restored. */
object TransferWorkerRuntime {
    private val dependencies = AtomicReference<TransferWorkerDependencies?>()

    fun install(value: TransferWorkerDependencies) {
        dependencies.set(value)
    }

    fun clear() {
        dependencies.set(null)
    }

    internal fun require(): TransferWorkerDependencies = dependencies.get()
        ?: throw IllegalStateException("TransferWorkerRuntime is not initialized")

    internal fun getOrNull(): TransferWorkerDependencies? = dependencies.get()
}

/** Manifest-declared, non-exported receiver used by the foreground notification cancel action. */
class TransferCancelReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_CANCEL_TRANSFER) return
        val jobId = intent.getStringExtra(TransferJobWorker.KEY_JOB_ID)?.takeIf(String::isNotBlank) ?: return
        val pendingResult = goAsync()
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            val runtime = TransferWorkerRuntime.getOrNull()
            val database = runtime?.database ?: TransferDatabase.build(context)
            try {
                TransferCancellation.markRequested(database.transferJobs(), jobId, runtime?.clock?.invoke() ?: System.currentTimeMillis())
                WorkManager.getInstance(context.applicationContext)
                    .cancelUniqueWork(TransferJobWorker.workName(jobId))
            } finally {
                if (runtime == null) database.close()
                pendingResult.finish()
            }
        }
    }

    companion object {
        const val ACTION_CANCEL_TRANSFER = "io.github.mith_mmk.wml2viewer.action.CANCEL_TRANSFER_V1"
    }
}

internal object TransferCancellation {
    suspend fun markRequested(dao: TransferJobDao, jobId: String, now: Long) {
        dao.requestCancel(jobId, now)
    }
}

internal suspend fun persistWorkerInterruption(
    dao: TransferJobDao,
    jobId: String,
    now: Long,
): Boolean {
    val userRequested = dao.isCancelRequested(jobId) == true
    if (userRequested) {
        dao.cancel(jobId, now)
    } else {
        dao.requeue(jobId, now)
    }
    return userRequested
}

class TransferJobWorker(
    appContext: Context,
    parameters: WorkerParameters,
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        val jobId = inputData.getString(KEY_JOB_ID) ?: return Result.failure(errorData("INVALID_JOB", "Missing job id"))
        val dependencies = try {
            TransferWorkerRuntime.require()
        } catch (error: IllegalStateException) {
            return Result.retry()
        }
        val dao = dependencies.database.transferJobs()
        val initial = dao.get(jobId) ?: return Result.failure(errorData("NOT_FOUND", "Transfer job not found"))
        if (initial.cancelRequested || initial.state == TransferJobState.CANCELLED) return Result.success()
        if (initial.state == TransferJobState.SUCCEEDED) return Result.success()
        if (initial.state == TransferJobState.FAILED) return Result.failure(errorData(initial.errorCode ?: "FAILED", ""))
        if (dao.claim(jobId, dependencies.clock()) != 1) return Result.retry()
        val job = dao.get(jobId) ?: return Result.failure(errorData("NOT_FOUND", "Transfer job not found"))

        try {
            val resumed = foregroundBeforeTransferResume(
                establishForeground = {
                    setForeground(createForegroundInfo(job, job.bytesCopied, job.totalBytes))
                },
                resume = { resumeClaimedJob(job, dependencies, dao) },
            )
            if (resumed != null) return resumed
            var lastPersistence = 0L
            var lastCancelCheck = 0L
            val transfer = dependencies.transferEngine.transfer(
                source = job.source,
                destinationParent = job.destinationParent,
                destinationName = job.destinationName,
                collisionPolicy = job.collisionPolicy,
                operation = job.operation,
                isCancelled = {
                    val elapsed = SystemClock.elapsedRealtime()
                    if (elapsed - lastCancelCheck >= CANCEL_CHECK_INTERVAL_MS) {
                        lastCancelCheck = elapsed
                        dao.isCancelRequested(jobId) == true
                    } else {
                        false
                    }
                },
                onProgress = { progress ->
                    val elapsed = SystemClock.elapsedRealtime()
                    if (elapsed - lastPersistence >= PROGRESS_INTERVAL_MS) {
                        lastPersistence = elapsed
                        dao.updateProgress(jobId, progress.bytesCopied, progress.totalBytes, dependencies.clock())
                        setProgress(workDataOf(KEY_BYTES_COPIED to progress.bytesCopied, KEY_TOTAL_BYTES to (progress.totalBytes ?: -1L)))
                        setForeground(createForegroundInfo(job, progress.bytesCopied, progress.totalBytes))
                    }
                },
                journal = object : TransferJournal {
                    override suspend fun copying(
                        temporary: io.github.mith_mmk.wml2viewer.data.source.EntryRef,
                        plannedFinalName: String,
                        replacementBackupName: String?,
                    ) {
                        dao.markCopying(
                            jobId,
                            temporary.providerId,
                            temporary.opaqueId,
                            plannedFinalName,
                            replacementBackupName,
                            dependencies.clock(),
                        )
                    }

                    override suspend fun verifying(verification: WriteVerification) {
                        dao.markVerifying(jobId, verification.byteCount, verification.sha256, dependencies.clock())
                    }

                    override suspend fun committed(destination: io.github.mith_mmk.wml2viewer.data.source.EntryRef) {
                        dao.markCommitted(jobId, destination.providerId, destination.opaqueId, dependencies.clock())
                    }

                    override suspend fun deletingSource() {
                        dao.markDeletingSource(jobId, dependencies.clock())
                    }
                },
            )
            if (transfer.skipped) {
                dao.succeed(jobId, 0, "", dependencies.clock())
            } else {
                dao.succeed(jobId, transfer.byteCount, transfer.sha256, dependencies.clock())
            }
            return Result.success(
                workDataOf(
                    KEY_BYTES_COPIED to transfer.byteCount,
                    KEY_SHA256 to transfer.sha256,
                    KEY_SKIPPED to transfer.skipped,
                ),
            )
        } catch (cancelled: CancellationException) {
            withContext(NonCancellable) {
                persistWorkerInterruption(dao, jobId, dependencies.clock())
            }
            throw cancelled
        } catch (error: SourceException) {
            val cancelRequested = dao.isCancelRequested(jobId) == true
            if (error.code == SourceErrorCode.CANCELLED && cancelRequested) {
                dao.cancel(jobId, dependencies.clock())
                return Result.success()
            }
            if (error.code == SourceErrorCode.CANCELLED) {
                dao.requeue(jobId, dependencies.clock())
                return Result.retry()
            }
            val message = SecretRedactor.redact(error.message)
            return if (error.retryable && runAttemptCount < MAX_WORK_RETRIES) {
                dao.requeue(jobId, dependencies.clock())
                Result.retry()
            } else {
                dao.fail(jobId, error.code.name, message, dependencies.clock())
                Result.failure(errorData(error.code.name, message))
            }
        } catch (error: Throwable) {
            val message = SecretRedactor.redact(error.message)
            dao.fail(jobId, "UNEXPECTED", message, dependencies.clock())
            return Result.failure(errorData("UNEXPECTED", message))
        }
    }

    private suspend fun resumeClaimedJob(
        job: TransferJobV1,
        dependencies: TransferWorkerDependencies,
        dao: TransferJobDao,
    ): Result? {
        return when (job.phase) {
        TransferJobPhase.COMMITTED, TransferJobPhase.DELETING_SOURCE -> {
            if (job.committed == null) {
                dao.fail(job.jobId, "JOURNAL_INVALID", "Committed destination is missing", dependencies.clock())
                Result.failure(errorData("JOURNAL_INVALID", ""))
            } else {
                if (job.operation == TransferOperation.MOVE) {
                    dao.markDeletingSource(job.jobId, dependencies.clock())
                    dependencies.transferEngine.deleteCommittedMoveSource(job.source)
                }
                dao.succeed(job.jobId, job.bytesCopied, job.sha256.orEmpty(), dependencies.clock())
                Result.success()
            }
        }
        TransferJobPhase.VERIFYING -> {
            recoverVerifyingJob(job, dependencies, dao)
        }
        TransferJobPhase.COPYING -> {
            job.staging?.let { dependencies.transferEngine.cleanupStaging(it) }
            dao.resetJournal(job.jobId, dependencies.clock())
            null
        }
        TransferJobPhase.DONE -> Result.success()
        TransferJobPhase.PREPARING -> null
        }
    }

    private suspend fun recoverVerifyingJob(
        job: TransferJobV1,
        dependencies: TransferWorkerDependencies,
        dao: TransferJobDao,
    ): Result? {
        val plannedFinalName = job.plannedFinalName
        val verification = job.sha256?.let { WriteVerification(job.bytesCopied, it) }
        if (plannedFinalName == null || verification == null) {
            dao.fail(job.jobId, "JOURNAL_INVALID", "Publication plan is missing", dependencies.clock())
            return Result.failure(errorData("JOURNAL_INVALID", ""))
        }

        val staging = job.staging
        val stagingEntry = staging?.let { dependencies.transferEngine.statOrNull(it) }
        val backupEntry = job.replacementBackupName?.let {
            dependencies.transferEngine.findEntryByName(job.destinationParent, it)
        }

        if (stagingEntry != null && stagingEntry.name != plannedFinalName) {
            val visibleFinal = dependencies.transferEngine.findEntryByName(
                job.destinationParent,
                plannedFinalName,
            )
            if (backupEntry != null) {
                if (visibleFinal != null) {
                    val recoveredBackupName = dependencies.transferEngine.statOrNull(
                        dependencies.transferEngine.surfaceReplacementBackup(
                            backup = backupEntry.ref,
                            plannedFinalName = plannedFinalName,
                            recoveryToken = job.jobId,
                        ),
                    )?.name
                    dao.fail(
                        job.jobId,
                        "RECOVERY_AMBIGUOUS",
                        if (recoveredBackupName == null) {
                            "Both replacement backup and destination are present"
                        } else {
                            "Both replacement backup and destination are present; previous data was recovered as " +
                                recoveredBackupName
                        },
                        dependencies.clock(),
                    )
                    return Result.failure(errorData("RECOVERY_AMBIGUOUS", ""))
                }
                dependencies.transferEngine.restoreReplacementBackup(
                    backupEntry.ref,
                    plannedFinalName,
                )
            }
            dependencies.transferEngine.cleanupStaging(stagingEntry.ref)
            dao.resetJournal(job.jobId, dependencies.clock())
            return null
        }

        val committed = when {
            stagingEntry != null && dependencies.transferEngine.verifyEntry(stagingEntry.ref, verification) ->
                stagingEntry.ref
            stagingEntry != null -> null
            else -> dependencies.transferEngine.findVerifiedDestination(
                job.destinationParent,
                plannedFinalName,
                verification,
            )
        }
        if (committed == null) {
            val visibleFinal = dependencies.transferEngine.findEntryByName(
                job.destinationParent,
                plannedFinalName,
            )
            var recoveredBackupName: String? = null
            if (backupEntry != null) {
                if (visibleFinal == null) {
                    dependencies.transferEngine.restoreReplacementBackup(
                        backupEntry.ref,
                        plannedFinalName,
                    )
                } else {
                    recoveredBackupName = dependencies.transferEngine.statOrNull(
                        dependencies.transferEngine.surfaceReplacementBackup(
                            backup = backupEntry.ref,
                            plannedFinalName = plannedFinalName,
                            recoveryToken = job.jobId,
                        ),
                    )?.name
                }
            }
            dao.fail(
                job.jobId,
                "RECOVERY_INTEGRITY",
                if (recoveredBackupName == null) {
                    "Published destination could not be verified"
                } else {
                    "Published destination could not be verified; previous data was recovered as $recoveredBackupName"
                },
                dependencies.clock(),
            )
            return Result.failure(errorData("RECOVERY_INTEGRITY", ""))
        }

        val replacementProven = stagingEntry != null || backupEntry != null
        if (job.operation == TransferOperation.MOVE &&
            job.collisionPolicy == CollisionPolicy.REPLACE && !replacementProven
        ) {
            // If the provider changed the staging ID and already removed its backup,
            // an old same-digest target is indistinguishable. Re-copy before deleting.
            dao.resetJournal(job.jobId, dependencies.clock())
            return null
        }
        backupEntry?.let { dependencies.transferEngine.cleanupStaging(it.ref) }
        dao.markCommitted(job.jobId, committed.providerId, committed.opaqueId, dependencies.clock())
        if (job.operation == TransferOperation.MOVE) {
            dao.markDeletingSource(job.jobId, dependencies.clock())
            dependencies.transferEngine.deleteCommittedMoveSource(job.source)
        }
        dao.succeed(job.jobId, job.bytesCopied, job.sha256.orEmpty(), dependencies.clock())
        return Result.success()
    }

    private fun createForegroundInfo(job: TransferJobV1, copied: Long, total: Long?): ForegroundInfo {
        createNotificationChannel()
        val percent = if (total != null && total > 0) ((copied * 100) / total).coerceIn(0, 100).toInt() else 0
        val notification = NotificationCompat.Builder(applicationContext, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(applicationContext.getString(R.string.transfer_notification_title))
            .setContentText(job.destinationName)
            .setOnlyAlertOnce(true)
            .setOngoing(true)
            .setProgress(100, percent, total == null || total <= 0)
            .addAction(
                android.R.drawable.ic_menu_close_clear_cancel,
                applicationContext.getString(R.string.transfer_notification_cancel),
                cancelPendingIntent(job.jobId),
            )
            .build()
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ForegroundInfo(notificationId(job.jobId), notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(notificationId(job.jobId), notification)
        }
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = applicationContext.getSystemService(Service.NOTIFICATION_SERVICE) as NotificationManager
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                applicationContext.getString(R.string.transfer_notification_channel),
                NotificationManager.IMPORTANCE_LOW,
            ),
        )
    }

    private fun notificationId(jobId: String) = NOTIFICATION_ID_BASE + (jobId.hashCode() and 0x0FFF)

    private fun cancelPendingIntent(jobId: String): PendingIntent = PendingIntent.getBroadcast(
        applicationContext,
        notificationId(jobId),
        Intent(applicationContext, TransferCancelReceiver::class.java)
            .setAction(TransferCancelReceiver.ACTION_CANCEL_TRANSFER)
            .putExtra(KEY_JOB_ID, jobId),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun errorData(code: String, message: String): Data = workDataOf(KEY_ERROR_CODE to code, KEY_ERROR_MESSAGE to message)

    companion object {
        const val KEY_JOB_ID = "transfer_job_id"
        const val KEY_BYTES_COPIED = "bytes_copied"
        const val KEY_TOTAL_BYTES = "total_bytes"
        const val KEY_SHA256 = "sha256"
        const val KEY_SKIPPED = "skipped"
        const val KEY_ERROR_CODE = "error_code"
        const val KEY_ERROR_MESSAGE = "error_message"
        private const val CHANNEL_ID = "file_transfers_v1"
        private const val NOTIFICATION_ID_BASE = 18_000
        private const val PROGRESS_INTERVAL_MS = 500L
        private const val CANCEL_CHECK_INTERVAL_MS = 250L
        private const val MAX_WORK_RETRIES = 3

        fun workName(jobId: String) = "transfer-v1-$jobId"

        fun request(jobId: String, requiresNetwork: Boolean) =
            OneTimeWorkRequestBuilder<TransferJobWorker>()
                .setInputData(workDataOf(KEY_JOB_ID to jobId))
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(if (requiresNetwork) NetworkType.CONNECTED else NetworkType.NOT_REQUIRED)
                        .build(),
                )
                .addTag("transfer-v1")
                .addTag(workName(jobId))
                .build()
    }
}

internal suspend fun <T> foregroundBeforeTransferResume(
    establishForeground: suspend () -> Unit,
    resume: suspend () -> T,
): T {
    establishForeground()
    return resume()
}

class TransferJobScheduler(
    context: Context,
    private val dao: TransferJobDao,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val applicationContext = context.applicationContext
    private val workManager: WorkManager by lazy(LazyThreadSafetyMode.SYNCHRONIZED) {
        WorkManager.getInstance(applicationContext)
    }

    suspend fun enqueue(job: TransferJobV1, requiresNetwork: Boolean) {
        dao.insert(job)
        enqueueExisting(job.jobId, requiresNetwork)
    }

    fun enqueueExisting(jobId: String, requiresNetwork: Boolean) {
        workManager.enqueueUniqueWork(
            TransferJobWorker.workName(jobId),
            ExistingWorkPolicy.KEEP,
            TransferJobWorker.request(jobId, requiresNetwork),
        )
    }

    suspend fun cancel(jobId: String) {
        dao.requestCancel(jobId, clock())
        workManager.cancelUniqueWork(TransferJobWorker.workName(jobId))
    }

    suspend fun recover(requiresNetwork: (TransferJobV1) -> Boolean) {
        dao.recoverCancelled(clock())
        dao.recoverInterrupted(clock())
        dao.queued().forEach { enqueueExisting(it.jobId, requiresNetwork(it)) }
    }
}
