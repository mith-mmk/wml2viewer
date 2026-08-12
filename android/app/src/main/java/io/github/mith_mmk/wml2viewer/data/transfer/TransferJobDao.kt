package io.github.mith_mmk.wml2viewer.data.transfer

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface TransferJobDao {
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(job: TransferJobV1)

    @Query("SELECT * FROM transfer_jobs_v1 WHERE job_id = :jobId")
    suspend fun get(jobId: String): TransferJobV1?

    @Query("SELECT * FROM transfer_jobs_v1 ORDER BY created_at_epoch_ms DESC")
    fun observeAll(): Flow<List<TransferJobV1>>

    @Query("SELECT * FROM transfer_jobs_v1 WHERE state = 'QUEUED' ORDER BY created_at_epoch_ms ASC")
    suspend fun queued(): List<TransferJobV1>

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'RUNNING', attempt_count = attempt_count + 1,
            started_at_epoch_ms = :now, updated_at_epoch_ms = :now,
            error_code = NULL, error_message = NULL
        WHERE job_id = :jobId AND state = 'QUEUED' AND cancel_requested = 0
        """,
    )
    suspend fun claim(jobId: String, now: Long): Int

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET bytes_copied = :copied, total_bytes = :total, updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun updateProgress(jobId: String, copied: Long, total: Long?, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET phase = 'COPYING', staging_provider_id = :providerId, staging_opaque_id = :opaqueId,
            planned_final_name = :plannedFinalName, replacement_backup_name = :replacementBackupName,
            updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun markCopying(
        jobId: String,
        providerId: String,
        opaqueId: String,
        plannedFinalName: String,
        replacementBackupName: String?,
        now: Long,
    )

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET phase = 'VERIFYING', bytes_copied = :copied, total_bytes = :copied,
            sha256 = :sha256, updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun markVerifying(jobId: String, copied: Long, sha256: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET phase = 'COMMITTED', committed_provider_id = :providerId, committed_opaque_id = :opaqueId,
            staging_provider_id = NULL, staging_opaque_id = NULL, planned_final_name = NULL,
            replacement_backup_name = NULL,
            updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun markCommitted(jobId: String, providerId: String, opaqueId: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1 SET phase = 'DELETING_SOURCE', updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun markDeletingSource(jobId: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET phase = 'PREPARING', staging_provider_id = NULL, staging_opaque_id = NULL,
            planned_final_name = NULL,
            replacement_backup_name = NULL,
            committed_provider_id = NULL, committed_opaque_id = NULL,
            bytes_copied = 0, total_bytes = NULL, sha256 = NULL, updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun resetJournal(jobId: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'SUCCEEDED', phase = 'DONE', bytes_copied = :copied, total_bytes = :copied,
            sha256 = :sha256, updated_at_epoch_ms = :now, completed_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING'
        """,
    )
    suspend fun succeed(jobId: String, copied: Long, sha256: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'FAILED', error_code = :errorCode, error_message = :errorMessage,
            updated_at_epoch_ms = :now, completed_at_epoch_ms = :now
        WHERE job_id = :jobId AND state IN ('QUEUED', 'RUNNING')
        """,
    )
    suspend fun fail(jobId: String, errorCode: String, errorMessage: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'QUEUED', updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state = 'RUNNING' AND cancel_requested = 0
        """,
    )
    suspend fun requeue(jobId: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET cancel_requested = 1, updated_at_epoch_ms = :now
        WHERE job_id = :jobId AND state IN ('QUEUED', 'RUNNING')
        """,
    )
    suspend fun requestCancel(jobId: String, now: Long)

    @Query("SELECT cancel_requested FROM transfer_jobs_v1 WHERE job_id = :jobId")
    suspend fun isCancelRequested(jobId: String): Boolean?

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'CANCELLED', cancel_requested = 1,
            updated_at_epoch_ms = :now, completed_at_epoch_ms = :now
        WHERE job_id = :jobId AND state IN ('QUEUED', 'RUNNING')
        """,
    )
    suspend fun cancel(jobId: String, now: Long)

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'CANCELLED', updated_at_epoch_ms = :now, completed_at_epoch_ms = :now
        WHERE state = 'RUNNING' AND cancel_requested = 1
        """,
    )
    suspend fun recoverCancelled(now: Long): Int

    @Query(
        """
        UPDATE transfer_jobs_v1
        SET state = 'QUEUED', updated_at_epoch_ms = :now
        WHERE state = 'RUNNING' AND cancel_requested = 0
        """,
    )
    suspend fun recoverInterrupted(now: Long): Int
}
