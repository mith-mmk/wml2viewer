package io.github.mith_mmk.wml2viewer.data.config

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.config.proto.SmbProfileV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import io.github.mith_mmk.wml2viewer.platform.security.CredentialInvalidatedException
import io.github.mith_mmk.wml2viewer.platform.security.CredentialInvalidationReason
import io.github.mith_mmk.wml2viewer.platform.security.CredentialSecret
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore
import kotlinx.coroutines.test.runTest
import org.junit.Test

class SavedSmbSourceLifecycleTest {
    @Test
    fun invalidatedCredentialIsDiscardedAndRequiresReentry() {
        val profile = smbProfile()
        val credentials = FakeCredentialStore().apply {
            invalidated = CredentialInvalidatedException(
                credentialId(profile.sourceId),
                CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING,
            )
        }
        val lifecycle = SavedSmbSourceLifecycle(FakeProfiles(profile), credentials)

        assertThat(lifecycle.credentialState(profile)).isEqualTo(SavedSmbCredentialState.REENTRY_REQUIRED)
        assertThat(credentials.deleted).containsExactly(profile.sourceId)
    }

    @Test
    fun replacementUsesSameSourceIdAndPersistsNoPlaintext() = runTest {
        val profile = smbProfile()
        val profiles = FakeProfiles(profile)
        val credentials = FakeCredentialStore()
        val lifecycle = SavedSmbSourceLifecycle(profiles, credentials)
        val password = "new-password".toCharArray()

        val updated = lifecycle.replaceCredential(profile.sourceId, password)

        assertThat(updated.sourceId).isEqualTo(profile.sourceId)
        assertThat(updated.smb.credentialId).isEqualTo(credentials.credentialId(profile.sourceId))
        assertThat(credentials.lastStoredPassword).isEqualTo(password)
        assertThat(SmbProfileV1::class.java.methods.map { it.name.lowercase() })
            .doesNotContain("getpassword")
    }

    @Test
    fun forgetDeletesCredentialAndProfile() = runTest {
        val profile = smbProfile()
        val profiles = FakeProfiles(profile)
        val credentials = FakeCredentialStore()

        assertThat(SavedSmbSourceLifecycle(profiles, credentials).forget(profile.sourceId)).isTrue()
        assertThat(credentials.deleted).containsExactly(profile.sourceId)
        assertThat(profiles.current()).isEmpty()
    }

    private fun smbProfile(): SourceProfileV1 {
        val sourceId = "saved-source"
        val credentialId = FakeCredentialStore().credentialId(sourceId)
        return SourceProfileV1.newBuilder()
            .setSourceId(sourceId)
            .setDisplayName("NAS/books")
            .setSmb(
                SmbProfileV1.newBuilder()
                    .setServer("nas.invalid")
                    .setPort(445)
                    .setShare("books")
                    .setUsername("reader")
                    .setCredentialId(credentialId),
            )
            .build()
    }
}

private class FakeProfiles(vararg initial: SourceProfileV1) : SavedSourceProfileStore {
    private val values = initial.associateByTo(linkedMapOf()) { it.sourceId }

    override suspend fun current(): List<SourceProfileV1> = values.values.toList()
    override suspend fun upsert(profile: SourceProfileV1) {
        values[profile.sourceId] = profile
    }
    override suspend fun remove(sourceId: String): Boolean = values.remove(sourceId) != null
}

private class FakeCredentialStore : CredentialStore {
    var invalidated: CredentialInvalidatedException? = null
    var lastStoredPassword: CharArray? = null
    val deleted = mutableListOf<String>()

    override fun put(profileId: String, password: CharArray) {
        lastStoredPassword = password.copyOf()
    }

    override fun load(profileId: String): CredentialSecret? {
        invalidated?.let { throw it }
        return null
    }

    override fun delete(profileId: String) {
        deleted += profileId
    }
}
