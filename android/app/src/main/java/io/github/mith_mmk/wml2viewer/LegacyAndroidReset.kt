package io.github.mith_mmk.wml2viewer

import android.content.Context
import android.content.Intent
import java.io.File

/** One-way cutover from the pre-0.0.19 NativeActivity snapshot implementation. */
internal object LegacyAndroidReset {
    private const val RESET_PREFERENCES = "android_v2_cutover"
    private const val RESET_KEY = "legacy_reset_complete"
    private const val GRANTS_CAPTURED_KEY = "legacy_grants_captured"
    private const val PENDING_GRANTS_KEY = "pending_legacy_grant_uris"
    private val legacyPaths = listOf(
        "imported",
        ".importing",
        "config",
        "picker.request",
        "import.ready",
    )

    fun runOnce(context: Context) {
        val resetState = context.getSharedPreferences(RESET_PREFERENCES, Context.MODE_PRIVATE)
        if (resetState.getBoolean(RESET_KEY, false)) return

        val resolver = context.contentResolver
        val currentPermissions = resolver.persistedUriPermissions
        val pendingGrantUris = pendingLegacyGrantUris(
            captured = resetState.getBoolean(GRANTS_CAPTURED_KEY, false),
            saved = resetState.getStringSet(PENDING_GRANTS_KEY, emptySet()).orEmpty(),
            current = currentPermissions.mapTo(linkedSetOf()) { it.uri.toString() },
        ).toMutableSet()
        if (!resetState.getBoolean(GRANTS_CAPTURED_KEY, false) &&
            !resetState.edit()
                .putStringSet(PENDING_GRANTS_KEY, pendingGrantUris)
                .putBoolean(GRANTS_CAPTURED_KEY, true)
                .commit()
        ) {
            return
        }
        currentPermissions.filter { it.uri.toString() in pendingGrantUris }.forEach { permission ->
            val flags = (if (permission.isReadPermission) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                (if (permission.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
            if (flags == 0 || runCatching {
                    resolver.releasePersistableUriPermission(permission.uri, flags)
                }.isSuccess
            ) {
                pendingGrantUris.remove(permission.uri.toString())
            }
        }
        val stillPersisted = resolver.persistedUriPermissions.mapTo(hashSetOf()) { it.uri.toString() }
        pendingGrantUris.removeAll { it !in stillPersisted }
        val grantStateSaved = resetState.edit()
            .putStringSet(PENDING_GRANTS_KEY, pendingGrantUris)
            .commit()

        var privateStateComplete = true
        legacyPaths.forEach { relativePath ->
            val target = File(context.filesDir, relativePath)
            if (target.exists()) {
                val deleted = if (target.isDirectory) target.deleteRecursively() else target.delete()
                if (!deleted || target.exists()) privateStateComplete = false
            }
        }
        if (!context.getSharedPreferences("storage", Context.MODE_PRIVATE).edit().clear().commit()) {
            privateStateComplete = false
        }
        if (grantStateSaved && pendingGrantUris.isEmpty() && privateStateComplete) {
            resetState.edit().putBoolean(RESET_KEY, true).commit()
        }
    }

    internal fun pathsForTest(): List<String> = legacyPaths.toList()
}

internal fun pendingLegacyGrantUris(
    captured: Boolean,
    saved: Set<String>,
    current: Set<String>,
): Set<String> = if (captured) saved.toSet() else current.toSet()
