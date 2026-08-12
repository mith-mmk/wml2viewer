package io.github.mith_mmk.wml2viewer.data.config

import com.google.common.truth.Truth.assertThat
import io.github.mith_mmk.wml2viewer.data.config.proto.SmbProfileV1
import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import org.junit.Test

class MobileSourceProfileStoreModelTest {
    @Test
    fun smbProfileContainsCredentialIdButNoPasswordField() {
        val profile = SourceProfileV1.newBuilder()
            .setSourceId("source-id")
            .setDisplayName("NAS")
            .setSmb(
                SmbProfileV1.newBuilder()
                    .setServer("nas.example")
                    .setPort(445)
                    .setShare("books")
                    .setUsername("reader")
                    .setCredentialId("one-way-id"),
            )
            .build()

        assertThat(profile.smb.credentialId).isEqualTo("one-way-id")
        val accessors = SmbProfileV1::class.java.methods.map { it.name.lowercase() }
        assertThat(accessors.any { name ->
            listOf("password", "passwd", "secret", "token").any(name::contains)
        }).isFalse()
    }
}
