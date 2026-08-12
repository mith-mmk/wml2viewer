package io.github.mith_mmk.wml2viewer.nativebridge

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class NativeRequestErrorTest {
    @Test
    fun safeArgumentsAcceptOnlyTypedBoundedValues() {
        val error = NativeRequestError(
            7,
            "native.limit",
            """{"dimension":"width","path":"/private/file","kind":"unexpected_eof"}""",
        )

        assertThat(error.safeArguments()).containsExactly("width", "unexpected_eof").inOrder()
    }

    @Test
    fun safeArgumentsRejectSecretsAndOversizedPayloads() {
        assertThat(
            NativeRequestError(1, null, """{"password":"hunter2"}""").safeArguments(),
        ).isEmpty()
        assertThat(
            NativeRequestError(1, null, "x".repeat(257)).safeArguments(),
        ).isEmpty()
    }
}
