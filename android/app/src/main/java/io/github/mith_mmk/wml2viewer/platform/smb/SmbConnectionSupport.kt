package io.github.mith_mmk.wml2viewer.platform.smb

import com.hierynomus.mssmb2.SMB2Dialect
import com.hierynomus.smbj.SmbConfig
import com.hierynomus.smbj.auth.AuthenticationContext
import com.hierynomus.smbj.connection.Connection
import com.hierynomus.smbj.session.Session
import io.github.mith_mmk.wml2viewer.data.source.SourceErrorCode
import io.github.mith_mmk.wml2viewer.data.source.SourceException
import io.github.mith_mmk.wml2viewer.platform.security.CredentialStore

/** Security-critical SMBJ configuration shared by disk and srvsvc connections. */
internal object SmbConnectionSupport {
    fun config(profile: SmbProfile): SmbConfig = SmbConfig.builder()
        .withDialects(
            SMB2Dialect.SMB_2_0_2,
            SMB2Dialect.SMB_2_1,
            SMB2Dialect.SMB_3_0,
            SMB2Dialect.SMB_3_0_2,
            SMB2Dialect.SMB_3_1_1,
        )
        .withMultiProtocolNegotiate(false)
        .withSigningEnabled(true)
        // SMBJ treats this as a client preference/capability. shouldEncryptData() remains
        // false when the negotiated SMB2/3 session has no encryption key, allowing a
        // visible plaintext fallback unless the profile requires encryption below.
        .withEncryptData(true)
        .build()

    /** Authentication must complete before CredentialSecret.use clears its password. */
    fun authenticate(
        connection: Connection,
        profile: SmbProfile,
        credentialStore: CredentialStore,
    ): Session = when (profile.authenticationMode) {
        SmbAuthenticationMode.GUEST -> connection.authenticate(AuthenticationContext.guest())
        SmbAuthenticationMode.USER_PASSWORD -> credentialStore.load(profile.profileId)?.use { secret ->
            connection.authenticate(AuthenticationContext(profile.username!!, secret.password, profile.domain))
        } ?: throw SourceException(SourceErrorCode.AUTHENTICATION_FAILED, "SMB credentials are unavailable")
    }

    fun securityStatus(connection: Connection, session: Session): SmbSecurityStatus {
        val dialect = connection.negotiatedProtocol.dialect
        SmbDialectPolicy.requireSmb2Or3(dialect.name)
        val encrypted = session.shouldEncryptData()
        val signingRequired = session.isSigningRequired
        val signingActive = session.sessionContext.signingKey != null
        return SmbSecurityStatus(
            connected = true,
            dialect = dialect.name,
            signingActive = signingActive,
            signingRequired = signingRequired,
            encryptionActive = encrypted,
            warnings = buildSet {
                if (!signingRequired) add(SmbSecurityWarning.SIGNING_NOT_REQUIRED)
                if (!encrypted) add(SmbSecurityWarning.NOT_ENCRYPTED)
                if (!dialect.isSmb3x && !encrypted) add(SmbSecurityWarning.SMB2_WITHOUT_ENCRYPTION)
            },
        )
    }
}
