package io.github.mith_mmk.wml2viewer.platform.smb

import com.google.common.truth.Truth.assertThat
import org.junit.Test
import java.io.IOException
import java.util.concurrent.TimeoutException

class SmbErrorClassifierTest {
    @Test
    fun wrappedIoFailureIsRetryable() {
        val failure = RuntimeException("SMB request failed", IOException("transport closed"))

        assertThat(failure.isRetryableSmbNetworkFailure()).isTrue()
    }

    @Test
    fun wrappedTimeoutIsRetryable() {
        val failure = RuntimeException(
            "SMB request failed",
            RuntimeException("transport failed", TimeoutException("timed out")),
        )

        assertThat(failure.isRetryableSmbNetworkFailure()).isTrue()
    }

    @Test
    fun unrelatedFailureIsNotRetryable() {
        val failure = RuntimeException("SMB request failed", IllegalStateException("invalid state"))

        assertThat(failure.isRetryableSmbNetworkFailure()).isFalse()
    }
}
