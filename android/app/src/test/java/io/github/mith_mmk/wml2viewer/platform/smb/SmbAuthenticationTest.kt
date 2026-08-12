package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class SmbAuthenticationTest {
    @Test
    fun guestModeUsesAnonymousAuthentication() {
        val context = SmbConnectionSupport.guestAuthenticationContext()

        assertThat(context.isAnonymous).isTrue()
        assertThat(context.isGuest).isFalse()
        assertThat(context.username).isEmpty()
    }
}
