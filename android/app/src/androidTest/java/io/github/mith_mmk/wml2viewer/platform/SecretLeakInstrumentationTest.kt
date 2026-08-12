package io.github.mith_mmk.wml2viewer.platform.security

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.InputStream

/** Mechanical guard that scans app-private persistent state for a CI-only sentinel secret. */
@RunWith(AndroidJUnit4::class)
class SecretLeakInstrumentationTest {
    @Test
    fun credentialPlaintextIsAbsentFromPrivatePersistentState() {
        val sentinel = InstrumentationRegistry.getArguments().getString(ARGUMENT_NAME)
        assumeTrue("Secret leak sentinel was not supplied", !sentinel.isNullOrEmpty())
        sentinel!!

        val context = ApplicationProvider.getApplicationContext<Context>()
        val store = KeystoreCredentialStore(context)
        val password = sentinel.toCharArray()
        try {
            store.put(PROFILE_ID, password)
        } finally {
            password.fill('\u0000')
        }

        try {
            val encodings = listOf(
                sentinel.toByteArray(Charsets.UTF_8),
                sentinel.toByteArray(Charsets.UTF_16LE),
            )
            val leakedFile = persistentRoots(context)
                .asSequence()
                .filter(File::exists)
                .flatMap { root -> root.walkTopDown().filter(File::isFile) }
                .firstOrNull { file -> encodings.any { needle -> file.containsBytes(needle) } }

            assertNull("A plaintext credential was found in app-private persistent state", leakedFile)
        } finally {
            store.delete(PROFILE_ID)
        }
    }

    private fun persistentRoots(context: Context): List<File> = listOfNotNull(
        context.filesDir,
        context.noBackupFilesDir,
        context.getDatabasePath("private-data-scan-anchor").parentFile,
        File(context.applicationInfo.dataDir, "shared_prefs"),
    ).distinctBy(File::getAbsolutePath)

    private fun File.containsBytes(needle: ByteArray): Boolean = runCatching {
        inputStream().buffered().use { input -> input.containsBytes(needle) }
    }.getOrDefault(false)

    private fun InputStream.containsBytes(needle: ByteArray): Boolean {
        require(needle.isNotEmpty())
        val prefix = IntArray(needle.size)
        var prefixLength = 0
        for (index in 1 until needle.size) {
            while (prefixLength > 0 && needle[index] != needle[prefixLength]) {
                prefixLength = prefix[prefixLength - 1]
            }
            if (needle[index] == needle[prefixLength]) prefixLength++
            prefix[index] = prefixLength
        }

        var matched = 0
        while (true) {
            val value = read()
            if (value < 0) return false
            val byte = value.toByte()
            while (matched > 0 && byte != needle[matched]) {
                matched = prefix[matched - 1]
            }
            if (byte == needle[matched]) matched++
            if (matched == needle.size) return true
        }
    }

    companion object {
        private const val ARGUMENT_NAME = "secretLeakSentinel"
        private const val PROFILE_ID = "instrumentation-secret-leak-probe"
    }
}
