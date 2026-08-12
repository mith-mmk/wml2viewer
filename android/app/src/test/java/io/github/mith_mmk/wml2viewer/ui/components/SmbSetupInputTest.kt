package io.github.mith_mmk.wml2viewer.ui.components

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.ui.model.SmbCredentialInput
import org.junit.Test

class SmbSetupInputTest {
    @Test
    fun finalShareInputRetainsIdentityAndOwnsReenteredPassword() {
        val input = buildSmbConnectionInput(
            server = " nas ",
            port = 445,
            share = " comics ",
            username = "alice",
            password = "second-entry",
            domain = "HOME",
            guest = false,
            requireEncryption = true,
            setupId = "setup-id",
        )

        try {
            assertThat(input.server).isEqualTo("nas")
            assertThat(input.share).isEqualTo("comics")
            assertThat(input.username).isEqualTo("alice")
            assertThat(input.domain).isEqualTo("HOME")
            assertThat(input.password.concatToString()).isEqualTo("second-entry")
            assertThat(input.setupId).isEqualTo("setup-id")
            assertThat(input.toString()).doesNotContain("second-entry")
            assertThat(input.toString()).doesNotContain("alice")
        } finally {
            input.clearPassword()
        }
        assertThat(input.password.all { it == '\u0000' }).isTrue()
    }

    @Test
    fun guestInputNeverCarriesIdentityOrPassword() {
        val input = buildSmbConnectionInput(
            server = "nas",
            port = 445,
            username = "ignored",
            password = "ignored",
            domain = "ignored",
            guest = true,
            requireEncryption = false,
        )

        assertThat(input.username).isEmpty()
        assertThat(input.domain).isEmpty()
        assertThat(input.password).isEmpty()
    }

    @Test
    fun savedCredentialInputRedactsAndZeroizesPassword() {
        val input = SmbCredentialInput("source-id", "replacement".toCharArray())

        assertThat(input.toString()).doesNotContain("source-id")
        assertThat(input.toString()).doesNotContain("replacement")
        input.clearPassword()
        assertThat(input.password.all { it == '\u0000' }).isTrue()
    }
}
