package io.github.mith_mmk.wml2viewer.ui.state

import io.github.mith_mmk.wml2viewer.nativebridge.NativeRequestError

/** Converts the stable native integer to a localizable UI identity; JSON is never rendered raw. */
fun NativeRequestError.toUiError(formattedArgs: List<String> = emptyList()): UiError = UiError(
    code = NativeUiErrorMapper.fromCode(code),
    args = formattedArgs,
)
