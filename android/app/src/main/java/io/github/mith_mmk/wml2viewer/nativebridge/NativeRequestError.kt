package io.github.mith_mmk.wml2viewer.nativebridge

/** Non-localized, privacy-safe diagnostic returned for a native request. */
data class NativeRequestError(
    val code: Int,
    val key: String?,
    val argsJson: String?,
) {
    /** Accepts only the bounded Rust ABI vocabulary; paths and arbitrary JSON never reach UI state. */
    fun safeArguments(): List<String> {
        val json = argsJson?.takeIf { it.length <= MAX_ARGS_JSON_LENGTH } ?: return emptyList()
        return SAFE_ARGUMENT.findAll(json)
            .map { it.groupValues[1] }
            .take(MAX_SAFE_ARGUMENTS)
            .toList()
    }

    companion object {
        private const val MAX_ARGS_JSON_LENGTH = 256
        private const val MAX_SAFE_ARGUMENTS = 4
        private val SAFE_ARGUMENT = Regex(
            "\\\"(?:argument|dimension|kind)\\\"\\s*:\\s*\\\"([a-z0-9_-]{1,32})\\\"",
        )

        internal fun read(sessionHandle: Long, requestId: Long): NativeRequestError? {
            val code = NativeBridge.requestErrorCode(sessionHandle, requestId)
            if (code == 0) return null
            return NativeRequestError(
                code = code,
                key = NativeBridge.requestErrorKey(sessionHandle, requestId),
                argsJson = NativeBridge.requestErrorArgsJson(sessionHandle, requestId),
            )
        }
    }
}
