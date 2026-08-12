package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.source.CollisionPolicy
import io.github.mith_mmk.wml2viewer.data.source.CreateRequest
import io.github.mith_mmk.wml2viewer.data.source.DeleteDisposition
import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.data.source.WriteVerification
import io.github.mith_mmk.wml2viewer.platform.security.CredentialSecret
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.security.MessageDigest
import java.util.UUID

/**
 * End-to-end SMB2/3 coverage against the ephemeral Samba server in CI.
 *
 * The environment gate keeps ordinary unit-test runs hermetic. CI generates the
 * password for each job, masks it, and never stores it in this repository.
 */
class SmbSambaIntegrationTest {
    @Test
    fun authenticatedAndGuestSharesSupportRealProviderOperations() = runBlocking {
        val environment = IntegrationEnvironment.fromEnvironment()
        assumeTrue("Samba integration environment is not configured", environment != null)
        environment!!

        val password = environment.password.toCharArray()
        val credentialStore = EphemeralCredentialStore()
        val authProfile = SmbProfile(
            profileId = "ci-auth-${UUID.randomUUID()}",
            server = environment.server,
            port = environment.port,
            share = environment.authShare,
            username = environment.username,
        )
        credentialStore.put(authProfile.profileId, password)
        password.fill('\u0000')

        try {
            verifyAuthenticatedOperations(authProfile, credentialStore, environment.expectedDialectPrefix)
            verifyGuestOperations(environment)
        } finally {
            credentialStore.close()
        }
    }

    private suspend fun verifyAuthenticatedOperations(
        profile: SmbProfile,
        credentialStore: CredentialStore,
        expectedDialectPrefix: String,
    ) {
        var provider = SmbSourceProvider(profile, credentialStore)
        val directoryName = "wml2viewer-${UUID.randomUUID()}"
        var directory: EntryRef? = null
        try {
            checkpoint("auth:create-directory")
            directory = provider.createDirectory(provider.root, directoryName, CollisionPolicy.FAIL)
            val largePayload = ByteArray(STREAM_BYTES) { index -> ((index * 31 + 17) and 0xff).toByte() }
            checkpoint("auth:write-original")
            val original = provider.writeVerified(directory, "payload.bin", largePayload)

            checkpoint("auth:list-original")
            assertThat(provider.list(directory).map { it.name }).containsExactly("payload.bin")
            checkpoint("auth:read-original")
            assertThat(provider.readBytesAndDigest(original)).isEqualTo(largePayload.sha256())

            checkpoint("auth:copy")
            val copied = provider.copy(original, directory, "copy.bin", CollisionPolicy.FAIL)
            checkpoint("auth:read-copy")
            assertThat(provider.readBytesAndDigest(copied)).isEqualTo(largePayload.sha256())

            checkpoint("auth:move")
            val moved = provider.move(copied, directory, "moved.bin", CollisionPolicy.FAIL)
            checkpoint("auth:list-after-move")
            assertThat(provider.list(directory).map { it.name }).doesNotContain("copy.bin")
            checkpoint("auth:rename")
            val renamed = provider.rename(moved, "renamed.bin", CollisionPolicy.FAIL)
            checkpoint("auth:stat-renamed")
            assertThat(provider.stat(renamed).name).isEqualTo("renamed.bin")

            checkpoint("auth:collision")
            val collision = runCatching {
                provider.create(directory, CreateRequest("payload.bin", collisionPolicy = CollisionPolicy.FAIL))
            }.exceptionOrNull()
            assertThat(collision).isInstanceOf(SourceException::class.java)
            assertThat((collision as SourceException).code).isEqualTo(SourceErrorCode.ALREADY_EXISTS)

            checkpoint("auth:keep-both")
            val kept = provider.writeVerified(directory, "payload.bin", byteArrayOf(1, 2, 3), CollisionPolicy.KEEP_BOTH)
            assertThat(provider.stat(kept).name).isEqualTo("payload (2).bin")

            val replacement = "replacement".encodeToByteArray()
            checkpoint("auth:replace")
            val replaced = provider.writeVerified(directory, "payload.bin", replacement, CollisionPolicy.REPLACE)
            checkpoint("auth:read-replacement")
            assertThat(provider.readBytesAndDigest(replaced)).isEqualTo(replacement.sha256())

            checkpoint("auth:close-first-provider")
            provider.close()
            provider = SmbSourceProvider(profile, credentialStore)
            checkpoint("auth:reconnect-list-root")
            val reconnectedDirectory = provider.list(provider.root).single { it.name == directoryName }.ref
            checkpoint("auth:reconnect-list-directory")
            val afterReconnect = provider.list(reconnectedDirectory).associateBy { it.name }
            checkpoint("auth:reconnect-read")
            assertThat(provider.readBytesAndDigest(afterReconnect.getValue("renamed.bin").ref))
                .isEqualTo(largePayload.sha256())
            assertThat(provider.securityStatus.value.dialect).startsWith("SMB_")
            assertThat(provider.securityStatus.value.dialect).startsWith(expectedDialectPrefix)
            SmbDialectPolicy.requireSmb2Or3(provider.securityStatus.value.dialect!!)

            afterReconnect.values.forEach {
                assertThat(provider.trashOrDelete(it.ref, allowPermanentDelete = true))
                    .isEqualTo(DeleteDisposition.PERMANENTLY_DELETED)
            }
            assertThat(provider.trashOrDelete(reconnectedDirectory, allowPermanentDelete = true))
                .isEqualTo(DeleteDisposition.PERMANENTLY_DELETED)
            directory = null
        } finally {
            directory?.let { cleanupDirectory(provider, it, directoryName) }
            provider.close()
        }
    }

    private suspend fun verifyGuestOperations(environment: IntegrationEnvironment) {
        val profile = SmbProfile(
            profileId = "ci-guest-${UUID.randomUUID()}",
            server = environment.server,
            port = environment.port,
            share = environment.guestShare,
            authenticationMode = SmbAuthenticationMode.GUEST,
        )
        val provider = SmbSourceProvider(profile, EmptyCredentialStore)
        val name = "guest-${UUID.randomUUID()}.txt"
        try {
            val bytes = "guest-write".encodeToByteArray()
            checkpoint("guest:write")
            val entry = provider.writeVerified(provider.root, name, bytes)
            checkpoint("guest:read")
            assertThat(provider.readBytesAndDigest(entry)).isEqualTo(bytes.sha256())
            checkpoint("guest:delete")
            assertThat(provider.trashOrDelete(entry, allowPermanentDelete = true))
                .isEqualTo(DeleteDisposition.PERMANENTLY_DELETED)
            checkpoint("guest:security-status")
            assertThat(provider.securityStatus.value.dialect).startsWith(environment.expectedDialectPrefix)
            SmbDialectPolicy.requireSmb2Or3(provider.securityStatus.value.dialect!!)
        } finally {
            checkpoint("guest:cleanup")
            runCatching { provider.list(provider.root) }.getOrDefault(emptyList()).firstOrNull { it.name == name }?.let {
                runCatching { provider.trashOrDelete(it.ref, allowPermanentDelete = true) }
            }
            provider.close()
        }
    }

    private suspend fun cleanupDirectory(provider: SmbSourceProvider, ref: EntryRef, name: String) {
        val reachable = runCatching { provider.stat(ref); ref }.getOrNull()
            ?: runCatching { provider.list(provider.root).firstOrNull { it.name == name }?.ref }.getOrNull()
        reachable ?: return
        runCatching { provider.list(reachable) }.getOrDefault(emptyList()).forEach {
            runCatching { provider.trashOrDelete(it.ref, allowPermanentDelete = true) }
        }
        runCatching { provider.trashOrDelete(reachable, allowPermanentDelete = true) }
    }

    private suspend fun SmbSourceProvider.writeVerified(
        parent: EntryRef,
        name: String,
        bytes: ByteArray,
        collisionPolicy: CollisionPolicy = CollisionPolicy.FAIL,
    ): EntryRef {
        val session = create(parent, CreateRequest(name, collisionPolicy = collisionPolicy))
        try {
            session.output.use { it.write(bytes) }
            assertThat(session.verify(WriteVerification(bytes.size.toLong(), bytes.sha256()))).isTrue()
            return session.commit()
        } catch (error: Throwable) {
            session.abort()
            throw error
        }
    }

    private suspend fun SmbSourceProvider.readBytesAndDigest(ref: EntryRef): String =
        openRead(ref).use { read -> read.stream.use { it.readBytes().sha256() } }

    private fun ByteArray.sha256(): String = MessageDigest.getInstance("SHA-256")
        .digest(this)
        .joinToString("") { "%02x".format(it) }

    private fun checkpoint(name: String) {
        println("SMB integration checkpoint: $name")
    }

    private data class IntegrationEnvironment(
        val server: String,
        val port: Int,
        val username: String,
        val password: String,
        val authShare: String,
        val guestShare: String,
        val expectedDialectPrefix: String,
    ) {
        companion object {
            fun fromEnvironment(): IntegrationEnvironment? {
                val password = System.getenv("WML2VIEWER_SMB_TEST_PASSWORD") ?: return null
                return IntegrationEnvironment(
                    server = System.getenv("WML2VIEWER_SMB_TEST_SERVER") ?: "127.0.0.1",
                    port = System.getenv("WML2VIEWER_SMB_TEST_PORT")?.toIntOrNull() ?: 1445,
                    username = System.getenv("WML2VIEWER_SMB_TEST_USERNAME") ?: "wml2ci",
                    password = password,
                    authShare = System.getenv("WML2VIEWER_SMB_TEST_AUTH_SHARE") ?: "auth",
                    guestShare = System.getenv("WML2VIEWER_SMB_TEST_GUEST_SHARE") ?: "guest",
                    expectedDialectPrefix = System.getenv("WML2VIEWER_SMB_TEST_EXPECTED_DIALECT_PREFIX")
                        ?: "SMB_",
                )
            }
        }
    }

    private class EphemeralCredentialStore : CredentialStore, AutoCloseable {
        private val values = mutableMapOf<String, CharArray>()

        override fun put(profileId: String, password: CharArray) {
            values.remove(profileId)?.fill('\u0000')
            values[profileId] = password.copyOf()
        }

        override fun load(profileId: String): CredentialSecret? =
            values[profileId]?.copyOf()?.let(::CredentialSecret)

        override fun delete(profileId: String) {
            values.remove(profileId)?.fill('\u0000')
        }

        override fun close() {
            values.values.forEach { it.fill('\u0000') }
            values.clear()
        }
    }

    private object EmptyCredentialStore : CredentialStore {
        override fun put(profileId: String, password: CharArray) = error("Guest mode has no credential")
        override fun load(profileId: String): CredentialSecret? = null
        override fun delete(profileId: String) = Unit
    }

    companion object {
        private const val STREAM_BYTES = 5 * 1024 * 1024
    }
}
