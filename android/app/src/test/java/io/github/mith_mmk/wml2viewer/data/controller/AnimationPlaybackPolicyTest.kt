package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import org.junit.Test

class AnimationPlaybackPolicyTest {
    @Test
    fun bufferingRejectsUnboundedFrameMemory() {
        assertThat(AnimationPlaybackPolicy.canBuffer(1_000, 1_000, 16)).isTrue()
        assertThat(AnimationPlaybackPolicy.canBuffer(4_000, 4_000, 3)).isFalse()
        assertThat(
            AnimationPlaybackPolicy.canBuffer(1, 1, AnimationPlaybackPolicy.MAX_BUFFERED_FRAMES + 1),
        ).isFalse()
    }

    @Test
    fun animationsTooLargeToBufferUseBoundedStreaming() {
        assertThat(AnimationPlaybackPolicy.storage(10, 10, 1))
            .isEqualTo(AnimationPlaybackPolicy.Storage.NONE)
        assertThat(AnimationPlaybackPolicy.storage(10, 10, 64))
            .isEqualTo(AnimationPlaybackPolicy.Storage.BUFFER)
        assertThat(AnimationPlaybackPolicy.storage(10, 10, 65))
            .isEqualTo(AnimationPlaybackPolicy.Storage.STREAM)
        assertThat(AnimationPlaybackPolicy.storage(4_000, 4_000, 3))
            .isEqualTo(AnimationPlaybackPolicy.Storage.STREAM)
    }

    @Test
    fun durationsAreSafeForCoroutinePlayback() {
        assertThat(AnimationPlaybackPolicy.durationMillis(-1)).isEqualTo(100)
        assertThat(AnimationPlaybackPolicy.durationMillis(0)).isEqualTo(100)
        assertThat(AnimationPlaybackPolicy.durationMillis(17)).isEqualTo(17)
        assertThat(AnimationPlaybackPolicy.durationMillis(90_000)).isEqualTo(60_000)
    }

    @Test
    fun loopCountsFollowNativeContract() {
        assertThat(AnimationPlaybackPolicy.playbackPasses(-1)).isEqualTo(1)
        assertThat(AnimationPlaybackPolicy.playbackPasses(3)).isEqualTo(3)
        assertThat(AnimationPlaybackPolicy.playbackPasses(0)).isEqualTo(Long.MAX_VALUE)
    }
}
