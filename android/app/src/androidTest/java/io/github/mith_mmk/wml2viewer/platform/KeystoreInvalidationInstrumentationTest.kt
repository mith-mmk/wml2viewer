package io.github.mith_mmk.wml2viewer.platform.security

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import java.security.KeyStore
import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@LargeTest
class KeystoreInvalidationInstrumentationTest {
    @Test
    fun deletingTheNonExportableKeyRequiresCredentialReentry() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val suffix = UUID.randomUUID().toString()
        val alias = "wml2viewer.test.credentials.$suffix"
        val profileId = "test-profile-$suffix"
        val store = KeystoreCredentialStore(context, keyAlias = alias)
        val password = suffix.toCharArray()
        try {
            store.put(profileId, password)
            password.fill('\u0000')
            KeyStore.getInstance("AndroidKeyStore").apply {
                load(null)
                deleteEntry(alias)
            }

            val invalidation = runCatching { store.load(profileId) }.exceptionOrNull()
            assertNotNull("Missing Keystore key must not silently produce a credential", invalidation)
            invalidation as CredentialInvalidatedException
            assertEquals(
                CredentialInvalidationReason.KEY_INVALIDATED_OR_MISSING,
                invalidation.reason,
            )
            store.discardInvalidated(profileId, invalidation)
        } finally {
            password.fill('\u0000')
            runCatching { store.delete(profileId) }
            runCatching {
                KeyStore.getInstance("AndroidKeyStore").apply {
                    load(null)
                    deleteEntry(alias)
                }
            }
        }
    }
}
