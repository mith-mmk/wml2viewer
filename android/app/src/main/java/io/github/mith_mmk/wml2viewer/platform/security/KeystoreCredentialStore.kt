package io.github.mith_mmk.wml2viewer.platform.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

interface CredentialStore {
    fun put(profileId: String, password: CharArray)
    fun load(profileId: String): CredentialSecret?
    fun delete(profileId: String)

    fun credentialId(profileId: String): String = credentialIdForProfile(profileId)

    fun discardInvalidated(profileId: String, invalidation: CredentialInvalidatedException): Boolean {
        require(invalidation.credentialId == credentialId(profileId)) { "Credential invalidation id mismatch" }
        delete(profileId)
        return true
    }
}

enum class CredentialInvalidationReason { KEY_INVALIDATED_OR_MISSING, CIPHERTEXT_TAMPERED, ENVELOPE_INVALID }

/** Contains only a one-way credential id; profile ids and secrets are intentionally excluded. */
class CredentialInvalidatedException(
    val credentialId: String,
    val reason: CredentialInvalidationReason,
    cause: Throwable? = null,
) : Exception("Stored credential must be entered again", cause)

class CredentialSecret internal constructor(val password: CharArray) : AutoCloseable {
    override fun close() {
        password.fill('\u0000')
    }

    override fun toString() = "CredentialSecret([REDACTED])"
}

/**
 * Passwords are encrypted with an Android Keystore AES-256-GCM key. Ciphertext
 * lives below noBackupFilesDir and is bound to the profile id using GCM AAD.
 */
class KeystoreCredentialStore(
    context: Context,
    private val keyAlias: String = DEFAULT_KEY_ALIAS,
    private val random: SecureRandom = SecureRandom(),
    private val keyProvider: (() -> SecretKey)? = null,
) : CredentialStore {
    private val directory = File(context.applicationContext.noBackupFilesDir, DIRECTORY_NAME)

    init {
        if (!directory.exists() && !directory.mkdirs()) {
            throw IllegalStateException("Unable to create credential directory")
        }
    }

    @Synchronized
    override fun put(profileId: String, password: CharArray) {
        validateCredentialProfileId(profileId)
        require(password.isNotEmpty()) { "password must not be empty" }
        val plaintext = password.toUtf8Bytes()
        try {
            val nonce = ByteArray(NONCE_SIZE).also(random::nextBytes)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey(), GCMParameterSpec(TAG_BITS, nonce))
            cipher.updateAAD(profileId.toByteArray(Charsets.UTF_8))
            val ciphertext = cipher.doFinal(plaintext)
            try {
                atomicWrite(fileFor(profileId), encodeEnvelope(nonce, ciphertext))
            } finally {
                ciphertext.fill(0)
            }
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    override fun load(profileId: String): CredentialSecret? {
        validateCredentialProfileId(profileId)
        val file = fileFor(profileId)
        if (!file.exists()) return null
        val credentialId = credentialId(profileId)
        val envelope = try {
            decodeEnvelope(file.readBytes())
        } catch (error: Throwable) {
            throw CredentialInvalidatedException(credentialId, CredentialInvalidationReason.ENVELOPE_INVALID, error)
        }
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, getExistingKey(), GCMParameterSpec(TAG_BITS, envelope.nonce))
            cipher.updateAAD(profileId.toByteArray(Charsets.UTF_8))
            val plaintext = cipher.doFinal(envelope.ciphertext)
            return try {
                CredentialSecret(plaintext.toUtf8Chars())
            } finally {
                plaintext.fill(0)
            }
        } catch (error: android.security.keystore.KeyPermanentlyInvalidatedException) {
            throw CredentialInvalidatedException(
                credentialId,
                CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING,
                error,
            )
        } catch (error: MissingKeystoreKeyException) {
            throw CredentialInvalidatedException(
                credentialId,
                CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING,
                error,
            )
        } catch (error: javax.crypto.AEADBadTagException) {
            throw CredentialInvalidatedException(credentialId, CredentialInvalidationReason.CIPHERTEXT_TAMPERED, error)
        } catch (error: java.security.InvalidKeyException) {
            throw CredentialInvalidatedException(
                credentialId,
                CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING,
                error,
            )
        } finally {
            envelope.nonce.fill(0)
            envelope.ciphertext.fill(0)
        }
    }

    @Synchronized
    override fun delete(profileId: String) {
        validateCredentialProfileId(profileId)
        val file = fileFor(profileId)
        if (file.exists() && !file.delete()) throw IllegalStateException("Unable to delete credential")
    }

    /**
     * Explicitly discards an unreadable envelope after UI has decided to ask for re-entry.
     * The one-way id check prevents an exception for one profile deleting another profile.
     */
    @Synchronized
    override fun discardInvalidated(profileId: String, invalidation: CredentialInvalidatedException): Boolean {
        validateCredentialProfileId(profileId)
        require(invalidation.credentialId == credentialId(profileId)) { "Credential invalidation id mismatch" }
        val file = fileFor(profileId)
        if (!file.exists()) return false
        if (!file.delete()) throw IllegalStateException("Unable to delete invalidated credential")
        return true
    }

    private fun getOrCreateKey(): SecretKey {
        keyProvider?.let { return it() }
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(keyAlias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE).run {
            init(
                KeyGenParameterSpec.Builder(
                    keyAlias,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setKeySize(KEY_BITS)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    // The nonce is generated with SecureRandom above and stored in
                    // the envelope. Android Keystore rejects caller-provided IVs
                    // when this flag is true (API 37), so explicitly allow the
                    // securely generated nonce here.
                    .setRandomizedEncryptionRequired(false)
                    .build(),
            )
            generateKey()
        }
    }

    private fun getExistingKey(): SecretKey {
        keyProvider?.let { return it() }
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        return keyStore.getKey(keyAlias, null) as? SecretKey ?: throw MissingKeystoreKeyException()
    }

    private fun fileFor(profileId: String): File {
        val name = credentialId(profileId)
        return File(directory, "$name.cred")
    }

    /** Stable one-way id suitable for re-entry UI and exception handling. */
    override fun credentialId(profileId: String): String = credentialIdForProfile(profileId)

    private fun atomicWrite(destination: File, bytes: ByteArray) {
        val temporary = File(directory, ".${destination.name}.${random.nextLong().toULong()}.tmp")
        try {
            FileOutputStream(temporary).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            try {
                Files.move(
                    temporary.toPath(),
                    destination.toPath(),
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(temporary.toPath(), destination.toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
        } finally {
            if (temporary.exists()) temporary.delete()
            bytes.fill(0)
        }
    }

    private fun encodeEnvelope(nonce: ByteArray, ciphertext: ByteArray): ByteArray =
        ByteArrayOutputStream().use { buffer ->
            DataOutputStream(buffer).use { output ->
                output.writeInt(MAGIC)
                output.writeByte(VERSION)
                output.writeByte(nonce.size)
                output.writeInt(ciphertext.size)
                output.write(nonce)
                output.write(ciphertext)
            }
            buffer.toByteArray()
        }

    private fun decodeEnvelope(bytes: ByteArray): Envelope {
        require(bytes.size <= MAX_ENVELOPE_SIZE) { "Credential envelope is too large" }
        return try {
            DataInputStream(ByteArrayInputStream(bytes)).use { input ->
                require(input.readInt() == MAGIC) { "Invalid credential envelope" }
                require(input.readUnsignedByte() == VERSION) { "Unsupported credential envelope" }
                val nonceLength = input.readUnsignedByte()
                val ciphertextLength = input.readInt()
                require(nonceLength == NONCE_SIZE) { "Invalid credential nonce" }
                require(ciphertextLength in 16..MAX_ENVELOPE_SIZE) { "Invalid credential ciphertext" }
                require(input.available() == nonceLength + ciphertextLength) { "Truncated credential envelope" }
                Envelope(ByteArray(nonceLength).also(input::readFully), ByteArray(ciphertextLength).also(input::readFully))
            }
        } finally {
            bytes.fill(0)
        }
    }

    private data class Envelope(val nonce: ByteArray, val ciphertext: ByteArray)

    companion object {
        const val DEFAULT_KEY_ALIAS = "wml2viewer.smb.credentials.v1"
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val DIRECTORY_NAME = "credentials-v1"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val KEY_BITS = 256
        private const val TAG_BITS = 128
        private const val NONCE_SIZE = 12
        private const val MAGIC = 0x574D4C32
        private const val VERSION = 1
        private const val MAX_ENVELOPE_SIZE = 64 * 1024
    }
}

fun credentialIdForProfile(profileId: String): String {
    validateCredentialProfileId(profileId)
    return MessageDigest.getInstance("SHA-256")
        .digest(profileId.toByteArray(Charsets.UTF_8))
        .joinToString("") { "%02x".format(it) }
}

private fun validateCredentialProfileId(profileId: String) {
    require(profileId.isNotBlank() && profileId.length <= 256) { "Invalid profile id" }
    require('\u0000' !in profileId) { "Invalid profile id" }
}

private class MissingKeystoreKeyException : Exception("Android Keystore key is unavailable")

object SecretRedactor {
    private val uriUserInfo = Regex("(?i)(smb://)([^/@\\s]+)@")
    private val namedSecret = Regex("(?i)(password|passwd|pwd|token|secret)(\\s*[=:]\\s*)([^,;\\s]+)")

    fun redact(message: String?, secrets: Iterable<CharArray> = emptyList()): String {
        if (message == null) return ""
        var redacted = uriUserInfo.replace(message, "$1[REDACTED]@")
        redacted = namedSecret.replace(redacted) { "${it.groupValues[1]}${it.groupValues[2]}[REDACTED]" }
        secrets.forEach { secret ->
            if (secret.isNotEmpty()) redacted = redacted.replace(String(secret), "[REDACTED]")
        }
        return redacted
    }
}

private fun CharArray.toUtf8Bytes(): ByteArray {
    val buffer = Charsets.UTF_8.newEncoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .encode(CharBuffer.wrap(this))
    return ByteArray(buffer.remaining()).also(buffer::get)
}

private fun ByteArray.toUtf8Chars(): CharArray {
    val buffer = Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(this))
    return CharArray(buffer.remaining()).also(buffer::get)
}
