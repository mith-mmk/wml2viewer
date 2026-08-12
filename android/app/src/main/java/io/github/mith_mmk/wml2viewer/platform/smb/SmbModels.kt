package io.github.mith_mmk.wml2viewer.platform.smb

import io.github.mith_mmk.wml2viewer.data.source.EntryRef
import java.nio.charset.StandardCharsets
import java.util.Base64

enum class SmbAuthenticationMode { GUEST, USER_PASSWORD }

data class SmbProfile(
    val profileId: String,
    val server: String,
    val port: Int = 445,
    val share: String? = null,
    val username: String? = null,
    val domain: String? = null,
    val authenticationMode: SmbAuthenticationMode = SmbAuthenticationMode.USER_PASSWORD,
    val requireEncryption: Boolean = false,
) {
    init {
        require(profileId.isNotBlank() && profileId.length <= 256) { "Invalid SMB profile id" }
        require(server.isNotBlank() && server.length <= 253 && '\u0000' !in server) { "Invalid SMB server" }
        require(port in 1..65_535) { "Invalid SMB port" }
        share?.let(SmbPathNormalizer::normalizeShare)
        if (authenticationMode == SmbAuthenticationMode.USER_PASSWORD) {
            require(!username.isNullOrBlank()) { "Username is required" }
        }
        require(username?.contains('\u0000') != true && domain?.contains('\u0000') != true) { "Invalid SMB identity" }
    }

    override fun toString(): String =
        "SmbProfile(profileId=[REDACTED], server=[REDACTED], port=$port, share=${share?.let { "[SET]" }}, username=${username?.let { "[SET]" }}, domain=${domain?.let { "[SET]" }}, authenticationMode=$authenticationMode, requireEncryption=$requireEncryption)"
}

enum class SmbSecurityWarning { SIGNING_NOT_REQUIRED, NOT_ENCRYPTED, SMB2_WITHOUT_ENCRYPTION }

data class SmbSecurityStatus(
    val connected: Boolean,
    val dialect: String?,
    val signingActive: Boolean,
    val signingRequired: Boolean,
    val encryptionActive: Boolean,
    val warnings: Set<SmbSecurityWarning>,
) {
    companion object {
        val DISCONNECTED = SmbSecurityStatus(false, null, false, false, false, emptySet())
    }
}

object SmbDialectPolicy {
    private val accepted = setOf("SMB_2_0_2", "SMB_2_1", "SMB_3_0", "SMB_3_0_2", "SMB_3_1_1")

    fun requireSmb2Or3(dialect: String) {
        require(dialect in accepted) { "SMB1 and unknown dialects are not supported" }
    }
}

data class SmbLocation(val share: String?, val path: String) {
    val isServerRoot: Boolean get() = share == null
}

object SmbPathNormalizer {
    private const val MAX_PATH_LENGTH = 32_000
    private const val MAX_SEGMENT_LENGTH = 255

    fun normalizeShare(share: String): String {
        val normalized = share.trim()
        require(normalized.isNotEmpty() && normalized.length <= MAX_SEGMENT_LENGTH) { "Invalid SMB share" }
        require(normalized != "." && normalized != "..") { "Invalid SMB share" }
        require(normalized.none { it == '\u0000' || it == '/' || it == '\\' }) { "Invalid SMB share" }
        return normalized
    }

    fun normalizePath(path: String): String {
        require(path.length <= MAX_PATH_LENGTH && '\u0000' !in path) { "Invalid SMB path" }
        if (path.isBlank()) return ""
        val segments = path.replace('/', '\\').split('\\').filter(String::isNotEmpty)
        require(segments.all { it != "." && it != ".." && it.length <= MAX_SEGMENT_LENGTH }) {
            "SMB path traversal is not allowed"
        }
        return segments.joinToString("\\")
    }

    fun child(parent: String, name: String): String {
        val segment = normalizePath(name)
        require(segment.isNotEmpty() && '\\' !in segment) { "Child name must be one segment" }
        val normalizedParent = normalizePath(parent)
        return if (normalizedParent.isEmpty()) segment else "$normalizedParent\\$segment"
    }

    fun parent(path: String): String? {
        val normalized = normalizePath(path)
        if (normalized.isEmpty()) return null
        return normalized.substringBeforeLast('\\', "")
    }

    fun name(path: String): String {
        val normalized = normalizePath(path)
        return normalized.substringAfterLast('\\', normalized)
    }

    fun sameEntry(left: SmbLocation, right: SmbLocation): Boolean =
        left.share.equals(right.share, ignoreCase = true) &&
            normalizePath(left.path).equals(normalizePath(right.path), ignoreCase = true)
}

internal object SmbOpaqueIdCodec {
    private const val PREFIX = "v1"
    private val encoder = Base64.getUrlEncoder().withoutPadding()
    private val decoder = Base64.getUrlDecoder()

    fun encode(location: SmbLocation): String =
        "$PREFIX.${encodePart(location.share.orEmpty())}.${encodePart(SmbPathNormalizer.normalizePath(location.path))}"

    fun decode(value: String): SmbLocation {
        val parts = value.split('.', limit = 3)
        require(parts.size == 3 && parts[0] == PREFIX) { "Unsupported SMB reference" }
        val share = decodePart(parts[1]).ifEmpty { null }?.let(SmbPathNormalizer::normalizeShare)
        return SmbLocation(share, SmbPathNormalizer.normalizePath(decodePart(parts[2])))
    }

    private fun encodePart(value: String): String = encoder.encodeToString(value.toByteArray(StandardCharsets.UTF_8))
    private fun decodePart(value: String): String = String(decoder.decode(value), StandardCharsets.UTF_8)
}

interface SmbShareEnumerationService {
    suspend fun enumerate(profile: SmbProfile): ShareDiscoveryResult
}

sealed interface ShareDiscoveryResult {
    data class Shares(val names: List<String>) : ShareDiscoveryResult
    data class ManualShareRequired(val reason: String) : ShareDiscoveryResult
}

/** SMBJ intentionally exposes shares after a name is known; RPC discovery is a replaceable service. */
object ManualShareEnumerationService : SmbShareEnumerationService {
    override suspend fun enumerate(profile: SmbProfile): ShareDiscoveryResult =
        ShareDiscoveryResult.ManualShareRequired("SRVSVC_MANUAL_SHARE_REQUIRED")
}

internal fun EntryRef.smbLocation(providerId: String): SmbLocation {
    require(this.providerId == providerId) { "Entry belongs to another provider" }
    return SmbOpaqueIdCodec.decode(opaqueId)
}
