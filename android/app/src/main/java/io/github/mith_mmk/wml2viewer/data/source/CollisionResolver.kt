package io.github.mith_mmk.wml2viewer.data.source

sealed interface CollisionResolution {
    data class Use(val name: String, val replaceExisting: Boolean) : CollisionResolution
    data object Skip : CollisionResolution
}

object CollisionResolver {
    private const val MAX_ATTEMPTS = 10_000

    suspend fun resolve(
        requestedName: String,
        policy: CollisionPolicy,
        exists: suspend (String) -> Boolean,
    ): CollisionResolution {
        validateFileName(requestedName)
        if (!exists(requestedName)) return CollisionResolution.Use(requestedName, replaceExisting = false)
        return when (policy) {
            CollisionPolicy.FAIL -> throw SourceException(
                SourceErrorCode.ALREADY_EXISTS,
                "An entry with the requested name already exists",
            )
            CollisionPolicy.REPLACE -> CollisionResolution.Use(requestedName, replaceExisting = true)
            CollisionPolicy.SKIP -> CollisionResolution.Skip
            CollisionPolicy.KEEP_BOTH -> CollisionResolution.Use(
                uniqueName(requestedName, exists),
                replaceExisting = false,
            )
        }
    }

    fun validateFileName(name: String) {
        if (
            name.isBlank() ||
            name == "." ||
            name == ".." ||
            '\u0000' in name ||
            '/' in name ||
            '\\' in name
        ) {
            throw SourceException(SourceErrorCode.INVALID_NAME, "Invalid entry name")
        }
    }

    private suspend fun uniqueName(name: String, exists: suspend (String) -> Boolean): String {
        val dot = name.lastIndexOf('.')
        val hasExtension = dot > 0 && dot < name.lastIndex
        val stem = if (hasExtension) name.substring(0, dot) else name
        val extension = if (hasExtension) name.substring(dot) else ""
        for (suffix in 2..MAX_ATTEMPTS) {
            val candidate = "$stem ($suffix)$extension"
            if (!exists(candidate)) return candidate
        }
        throw SourceException(SourceErrorCode.ALREADY_EXISTS, "Unable to allocate a unique name")
    }
}
