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
        // Anonymous sessions have no key for SMB3 encryption. Authenticated sessions
        // prefer encryption; requireEncryption is enforced after session negotiation.
        .withEncryptData(profile.authenticationMode != SmbAuthenticationMode.GUEST)
        .build()

    /** Authentication must complete before CredentialSecret.use clears its password. */
    fun authenticate(
        connection: Connection,
        profile: SmbProfile,
        credentialStore: CredentialStore,
    ): Session = when (profile.authenticationMode) {
        SmbAuthenticationMode.GUEST -> connection.authenticate(
            guestAuthenticationContext(connection.negotiatedProtocol.dialect),
        )
        SmbAuthenticationMode.USER_PASSWORD -> credentialStore.load(profile.profileId)?.use { secret ->
            connection.authenticate(AuthenticationContext(profile.username!!, secret.password, profile.domain))
        } ?: throw SourceException(SourceErrorCode.AUTHENTICATION_FAILED, "SMB credentials are unavailable")
    }

    /**
     * Samba SMB2 maps anonymous sessions reliably, while SMB3 must carry the guest
     * identity so SMBJ does not derive signing keys from an empty session key.
     */
    internal fun guestAuthenticationContext(dialect: SMB2Dialect): AuthenticationContext =
        if (dialect.isSmb3x) AuthenticationContext.guest() else AuthenticationContext.anonymous()

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
