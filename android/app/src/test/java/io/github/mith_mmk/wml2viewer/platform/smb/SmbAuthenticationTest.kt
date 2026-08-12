package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import com.hierynomus.mssmb2.SMB2Dialect
import org.junit.Test

class SmbAuthenticationTest {
    @Test
    fun smb2GuestModeUsesAnonymousAuthentication() {
        val context = SmbConnectionSupport.guestAuthenticationContext(SMB2Dialect.SMB_2_1)

        assertThat(context.isAnonymous).isTrue()
        assertThat(context.isGuest).isFalse()
        assertThat(context.username).isEmpty()
    }

    @Test
    fun smb3GuestModeUsesGuestAuthentication() {
        val context = SmbConnectionSupport.guestAuthenticationContext(SMB2Dialect.SMB_3_1_1)

        assertThat(context.isAnonymous).isFalse()
        assertThat(context.isGuest).isTrue()
        assertThat(context.username).isEqualTo("Guest")
    }
}
