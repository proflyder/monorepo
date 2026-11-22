package dev.proflyder.home.core.ui

import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.ForegroundState
import dev.proflyder.home.core.testing.FakeClock
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ForegroundGraceNoticePolicyTest {
    private val clock = FakeClock(elapsedNanos = 100L)
    private val foreground = object : AppForeground {
        override val state = MutableStateFlow(ForegroundState(foreground = true, sinceElapsedNanos = 100L))
    }
    private val policy = ForegroundGraceNoticePolicy(foreground, clock)

    @Test
    fun transientFailuresAreHiddenRightAfterReturningToForeground() {
        assertFalse(policy.shouldShow(RemoteFailure.Timeout))
        assertTrue(policy.shouldShow(RemoteFailure.Unauthorized))
    }

    @Test
    fun transientFailuresShowOnceTheGraceIsOver() {
        clock.elapsedNanos += ForegroundGraceNoticePolicy.GRACE_NANOS + 1
        assertTrue(policy.shouldShow(RemoteFailure.Offline("down")))
    }

    @Test
    fun transientFailuresShowInBackground() {
        foreground.state.value = ForegroundState(foreground = false, sinceElapsedNanos = 100L)
        assertTrue(policy.shouldShow(RemoteFailure.Timeout))
    }
}
