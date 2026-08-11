package io.github.mith_mmk.wml2viewer.data.config

import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import io.github.mith_mmk.wml2viewer.platform.security.CredentialInvalidatedException
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore

enum class SavedSmbCredentialState { READY, REENTRY_REQUIRED }

/** Keeps saved SMB descriptors and encrypted credentials synchronized without persisting plaintext. */
class SavedSmbSourceLifecycle(
    private val profiles: SavedSourceProfileStore,
    private val credentials: CredentialStore,
) {
    fun credentialState(profile: SourceProfileV1): SavedSmbCredentialState {
        require(profile.sourceCase == SourceProfileV1.SourceCase.SMB) { "SMB profile is required" }
        if (profile.smb.guest) return SavedSmbCredentialState.READY
        val expectedId = credentials.credentialId(profile.sourceId)
        if (profile.smb.credentialId != expectedId) {
            credentials.delete(profile.sourceId)
            return SavedSmbCredentialState.REENTRY_REQUIRED
        }
        return try {
            val stored = credentials.load(profile.sourceId)
            if (stored == null) {
                SavedSmbCredentialState.REENTRY_REQUIRED
            } else {
                stored.close()
                SavedSmbCredentialState.READY
            }
        } catch (invalidated: CredentialInvalidatedException) {
            credentials.discardInvalidated(profile.sourceId, invalidated)
            SavedSmbCredentialState.REENTRY_REQUIRED
        }
    }

    suspend fun replaceCredential(sourceId: String, password: CharArray): SourceProfileV1 {
        require(password.isNotEmpty()) { "SMB password is required" }
        val profile = profiles.current().singleOrNull { it.sourceId == sourceId }
            ?: throw IllegalArgumentException("Saved SMB source is unavailable")
        require(profile.sourceCase == SourceProfileV1.SourceCase.SMB && !profile.smb.guest) {
            "Saved source does not accept a password"
        }
        credentials.put(sourceId, password)
        val updated = profile.toBuilder()
            .setSmb(
                profile.smb.toBuilder()
                    .setCredentialId(credentials.credentialId(sourceId)),
            )
            .build()
        profiles.upsert(updated)
        return updated
    }

    suspend fun forget(sourceId: String): Boolean {
        var removed = false
        try {
            credentials.delete(sourceId)
        } finally {
            removed = profiles.remove(sourceId)
        }
        return removed
    }
}
