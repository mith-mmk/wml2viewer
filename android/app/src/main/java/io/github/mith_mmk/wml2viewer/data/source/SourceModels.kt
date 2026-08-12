package io.github.mith_mmk.wml2viewer.data.source

import java.io.InputStream
import java.io.OutputStream

/** Stable, provider-owned reference. [opaqueId] must never contain credentials. */
data class EntryRef(
    val providerId: String,
    val opaqueId: String,
) {
    init {
        require(providerId.isNotBlank()) { "providerId must not be blank" }
        require(opaqueId.isNotBlank()) { "opaqueId must not be blank" }
        require(providerId.length <= MAX_COMPONENT_LENGTH) { "providerId is too long" }
        require(opaqueId.length <= MAX_OPAQUE_ID_LENGTH) { "opaqueId is too long" }
        require('\u0000' !in providerId && '\u0000' !in opaqueId) { "EntryRef contains NUL" }
    }

    companion object {
        const val MAX_COMPONENT_LENGTH = 256
        const val MAX_OPAQUE_ID_LENGTH = 8_192
    }
}

enum class EntryKind { FILE, DIRECTORY }

data class SourceCapabilities(
    val canList: Boolean = true,
    val canRead: Boolean = true,
    val canCreate: Boolean = false,
    val canCopyWithinProvider: Boolean = false,
    val canMoveWithinProvider: Boolean = false,
    val canRename: Boolean = false,
    val canTrash: Boolean = false,
    val canDelete: Boolean = false,
    val canThumbnail: Boolean = false,
    val hasAtomicFinalize: Boolean = false,
    val canCopyDirectoriesWithinProvider: Boolean = false,
    val canMoveDirectoriesWithinProvider: Boolean = false,
    val canTransferDirectoriesAcrossProviders: Boolean = false,
)

data class SourceEntry(
    val ref: EntryRef,
    val parent: EntryRef?,
    val name: String,
    val kind: EntryKind,
    val mimeType: String?,
    val size: Long?,
    val modifiedAtEpochMillis: Long?,
    val isHidden: Boolean = false,
    /** Entry-specific capabilities. UI and operations must use these over provider-wide support. */
    val effectiveCapabilities: SourceCapabilities = SourceCapabilities(),
    /** Provider-native flags retained for diagnostics; never interpret across providers. */
    val platformFlags: Long? = null,
)

data class SourceRead(
    val stream: InputStream,
    val size: Long?,
    val mimeType: String?,
) : AutoCloseable {
    override fun close() = stream.close()
}

data class SourceThumbnail(
    val encodedBytes: ByteArray,
    val mimeType: String,
    val width: Int,
    val height: Int,
) {
    init {
        require(encodedBytes.isNotEmpty()) { "thumbnail must not be empty" }
        require(width > 0 && height > 0) { "thumbnail dimensions must be positive" }
    }

    override fun equals(other: Any?): Boolean =
        other is SourceThumbnail &&
            encodedBytes.contentEquals(other.encodedBytes) &&
            mimeType == other.mimeType &&
            width == other.width &&
            height == other.height

    override fun hashCode(): Int =
        31 * (31 * (31 * encodedBytes.contentHashCode() + mimeType.hashCode()) + width) + height
}

enum class CollisionPolicy { FAIL, REPLACE, KEEP_BOTH, SKIP }

data class CreateRequest(
    val name: String,
    val mimeType: String = "application/octet-stream",
    val collisionPolicy: CollisionPolicy = CollisionPolicy.FAIL,
    val kind: EntryKind = EntryKind.FILE,
)

data class WriteVerification(
    val byteCount: Long,
    val sha256: String,
)

/** A temporary write which is published only by [commit]. [abort] is idempotent. */
interface AtomicWriteSession : AutoCloseable {
    val temporaryRef: EntryRef
    /** Exact leaf name selected after applying the collision policy. */
    val plannedFinalName: String
    /** Exact hidden backup name used while publishing a replacement, when required. */
    val replacementBackupName: String?
    val output: OutputStream
    val skippedExistingRef: EntryRef? get() = null

    suspend fun verify(expected: WriteVerification): Boolean
    suspend fun commit(): EntryRef
    suspend fun abort()

    override fun close() {
        output.close()
    }
}

enum class DeleteDisposition { TRASHED, PERMANENTLY_DELETED }

enum class SourceErrorCode {
    NOT_FOUND,
    ALREADY_EXISTS,
    ACCESS_DENIED,
    AUTHENTICATION_FAILED,
    UNSUPPORTED,
    INVALID_REFERENCE,
    INVALID_NAME,
    NETWORK,
    INTEGRITY,
    CANCELLED,
    IO,
}

class SourceException(
    val code: SourceErrorCode,
    message: String,
    cause: Throwable? = null,
    val retryable: Boolean = false,
) : Exception(message, cause)
