package io.github.mith_mmk.wml2viewer.data.transfer

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import androidx.room.TypeConverter
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.TransferOperation
import java.util.UUID

enum class TransferJobState { QUEUED, RUNNING, SUCCEEDED, FAILED, CANCELLED }
enum class TransferJobPhase { PREPARING, COPYING, VERIFYING, COMMITTED, DELETING_SOURCE, DONE }

@Entity(
    tableName = "transfer_jobs_v1",
    indices = [
        Index(value = ["state", "created_at_epoch_ms"]),
        Index(value = ["updated_at_epoch_ms"]),
    ],
)
data class TransferJobV1(
    @PrimaryKey
    @ColumnInfo(name = "job_id")
    val jobId: String,
    @ColumnInfo(name = "source_provider_id")
    val sourceProviderId: String,
    @ColumnInfo(name = "source_opaque_id")
    val sourceOpaqueId: String,
    @ColumnInfo(name = "destination_provider_id")
    val destinationProviderId: String,
    @ColumnInfo(name = "destination_parent_opaque_id")
    val destinationParentOpaqueId: String,
    @ColumnInfo(name = "destination_name")
    val destinationName: String,
    @ColumnInfo(name = "operation")
    val operation: TransferOperation,
    @ColumnInfo(name = "collision_policy")
    val collisionPolicy: CollisionPolicy,
    @ColumnInfo(name = "state")
    val state: TransferJobState,
    @ColumnInfo(name = "phase")
    val phase: TransferJobPhase,
    @ColumnInfo(name = "staging_provider_id")
    val stagingProviderId: String?,
    @ColumnInfo(name = "staging_opaque_id")
    val stagingOpaqueId: String?,
    @ColumnInfo(name = "planned_final_name")
    val plannedFinalName: String?,
    @ColumnInfo(name = "replacement_backup_name")
    val replacementBackupName: String?,
    @ColumnInfo(name = "committed_provider_id")
    val committedProviderId: String?,
    @ColumnInfo(name = "committed_opaque_id")
    val committedOpaqueId: String?,
    @ColumnInfo(name = "bytes_copied")
    val bytesCopied: Long,
    @ColumnInfo(name = "total_bytes")
    val totalBytes: Long?,
    @ColumnInfo(name = "sha256")
    val sha256: String?,
    @ColumnInfo(name = "attempt_count")
    val attemptCount: Int,
    @ColumnInfo(name = "cancel_requested")
    val cancelRequested: Boolean,
    @ColumnInfo(name = "error_code")
    val errorCode: String?,
    @ColumnInfo(name = "error_message")
    val errorMessage: String?,
    @ColumnInfo(name = "created_at_epoch_ms")
    val createdAtEpochMillis: Long,
    @ColumnInfo(name = "updated_at_epoch_ms")
    val updatedAtEpochMillis: Long,
    @ColumnInfo(name = "started_at_epoch_ms")
    val startedAtEpochMillis: Long?,
    @ColumnInfo(name = "completed_at_epoch_ms")
    val completedAtEpochMillis: Long?,
) {
    val source: EntryRef get() = EntryRef(sourceProviderId, sourceOpaqueId)
    val destinationParent: EntryRef get() = EntryRef(destinationProviderId, destinationParentOpaqueId)
    val staging: EntryRef? get() = stagingProviderId?.let { provider -> stagingOpaqueId?.let { EntryRef(provider, it) } }
    val committed: EntryRef? get() = committedProviderId?.let { provider -> committedOpaqueId?.let { EntryRef(provider, it) } }

    companion object {
        fun create(
            source: EntryRef,
            destinationParent: EntryRef,
            destinationName: String,
            operation: TransferOperation,
            collisionPolicy: CollisionPolicy,
            nowEpochMillis: Long = System.currentTimeMillis(),
            jobId: String = UUID.randomUUID().toString(),
        ) = TransferJobV1(
            jobId = jobId,
            sourceProviderId = source.providerId,
            sourceOpaqueId = source.opaqueId,
            destinationProviderId = destinationParent.providerId,
            destinationParentOpaqueId = destinationParent.opaqueId,
            destinationName = destinationName,
            operation = operation,
            collisionPolicy = collisionPolicy,
            state = TransferJobState.QUEUED,
            phase = TransferJobPhase.PREPARING,
            stagingProviderId = null,
            stagingOpaqueId = null,
            plannedFinalName = null,
            replacementBackupName = null,
            committedProviderId = null,
            committedOpaqueId = null,
            bytesCopied = 0,
            totalBytes = null,
            sha256 = null,
            attemptCount = 0,
            cancelRequested = false,
            errorCode = null,
            errorMessage = null,
            createdAtEpochMillis = nowEpochMillis,
            updatedAtEpochMillis = nowEpochMillis,
            startedAtEpochMillis = null,
            completedAtEpochMillis = null,
        )
    }
}

class TransferJobConverters {
    @TypeConverter fun operationToString(value: TransferOperation): String = value.name
    @TypeConverter fun stringToOperation(value: String): TransferOperation = TransferOperation.valueOf(value)
    @TypeConverter fun collisionToString(value: CollisionPolicy): String = value.name
    @TypeConverter fun stringToCollision(value: String): CollisionPolicy = CollisionPolicy.valueOf(value)
    @TypeConverter fun stateToString(value: TransferJobState): String = value.name
    @TypeConverter fun stringToState(value: String): TransferJobState = TransferJobState.valueOf(value)
    @TypeConverter fun phaseToString(value: TransferJobPhase): String = value.name
    @TypeConverter fun stringToPhase(value: String): TransferJobPhase = TransferJobPhase.valueOf(value)
}
