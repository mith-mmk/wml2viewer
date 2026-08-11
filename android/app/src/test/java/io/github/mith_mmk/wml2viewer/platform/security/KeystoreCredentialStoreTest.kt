package io.github.mith_mmk.wml2viewer.platform.security

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import com.google.common.truth.Truth.assertThat
import org.junit.After
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.io.File
import java.security.InvalidKeyException
import javax.crypto.spec.SecretKeySpec

@RunWith(RobolectricTestRunner::class)
class KeystoreCredentialStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val directory get() = File(context.noBackupFilesDir, "credentials-v1")
    private val key = SecretKeySpec(ByteArray(32) { it.toByte() }, "AES")

    @Before fun setUp() { directory.deleteRecursively() }
    @After fun tearDown() { directory.deleteRecursively() }

    @Test
    fun roundTripHashesFileNameAndZeroizesSecret() {
        val store = store()
        val password = "correct horse battery staple".toCharArray()
        store.put("private-profile", password)
        assertThat(directory.listFiles().orEmpty().single().name).doesNotContain("private-profile")
        val secret = store.load("private-profile")!!
        assertThat(String(secret.password)).isEqualTo(String(password))
        secret.close()
        assertThat(secret.password.all { it == '\u0000' }).isTrue()
    }

    @Test
    fun tamperIsTypedAndCiphertextDeletionIsExplicit() {
        val store = store()
        store.put("profile", "password".toCharArray())
        val file = File(directory, "${store.credentialId("profile")}.cred")
        file.writeBytes(file.readBytes().also { it[it.lastIndex] = (it.last().toInt() xor 1).toByte() })
        val invalid = assertThrows(CredentialInvalidatedException::class.java) { store.load("profile") }
        assertThat(invalid.reason).isEqualTo(CredentialInvalidationReason.CIPHERTEXT_TAMPERED)
        assertThat(invalid.message).doesNotContain("profile")
        assertThat(store.discardInvalidated("profile", invalid)).isTrue()
        assertThat(file.exists()).isFalse()
    }

    @Test
    fun malformedEnvelopeAndInvalidatedKeyHaveTypedReasons() {
        val store = store()
        directory.mkdirs()
        File(directory, "${store.credentialId("broken")}.cred").writeBytes(byteArrayOf(1, 2, 3))
        assertThat(assertThrows(CredentialInvalidatedException::class.java) { store.load("broken") }.reason)
            .isEqualTo(CredentialInvalidationReason.ENVELOPE_INVALID)
        store.put("invalid-key", "password".toCharArray())
        val invalidKeyStore = KeystoreCredentialStore(context, keyProvider = { throw InvalidKeyException("invalidated") })
        assertThat(assertThrows(CredentialInvalidatedException::class.java) { invalidKeyStore.load("invalid-key") }.reason)
            .isEqualTo(CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING)
    }

    @Test
    fun redactorRemovesUriAndNamedSecrets() {
        val value = SecretRedactor.redact("smb://alice:pw@nas/share password=hunter2 token:abcd")
        assertThat(value).doesNotContain("alice:pw")
        assertThat(value).doesNotContain("hunter2")
        assertThat(value).doesNotContain("abcd")
    }

    private fun store() = KeystoreCredentialStore(context, keyProvider = { key })
}
