package dev.proflyder.home.core.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ServerClockTest {
    private class FakeTime : Clock {
        var wallMillis: Long = 5_000_000L
        var elapsedNanos: Long = 0L

        override fun wallMillis(): Long = wallMillis

        override fun elapsedNanos(): Long = elapsedNanos

        fun advanceSeconds(seconds: Long) {
            wallMillis += seconds * 1_000L
            elapsedNanos += seconds * 1_000_000_000L
        }
    }

    @Test
    fun wallClockFallbackIsUsedOnlyBeforeSync() {
        val time = FakeTime()
        val clock = ServerClock(time)
        assertFalse(clock.isSynced)
        assertEquals(5_000L, clock.nowUnix())
        time.wallMillis += 30_000L
        assertEquals(5_030L, clock.nowUnix())
    }

    @Test
    fun syncUsesServerTimestampAndMonotonicElapsedTime() {
        val time = FakeTime()
        val clock = ServerClock(time)

        clock.sync(5_090L)
        assertTrue(clock.isSynced)
        assertEquals(5_090L, clock.nowUnix())
        assertEquals(90L, clock.offsetSeconds())

        time.advanceSeconds(10)
        assertEquals(5_100L, clock.nowUnix())
        assertEquals(90L, clock.offsetSeconds())
    }

    @Test
    fun wallClockCorrectionAfterSyncDoesNotJumpCountdown() {
        val time = FakeTime()
        val clock = ServerClock(time)
        clock.sync(5_000L)

        val before = clock.nowUnix()

        time.wallMillis += 3_600_000L
        assertEquals(before, clock.nowUnix())

        time.wallMillis -= 7_200_000L
        assertEquals(before, clock.nowUnix())

        assertEquals(before - time.wallMillis / 1000L, clock.offsetSeconds())
    }

    @Test
    fun elapsedTimeAdvancesCountdownAndResyncCorrectsDrift() {
        val time = FakeTime()
        val clock = ServerClock(time)
        clock.sync(5_000L)

        time.elapsedNanos += 7_500_000_000L
        assertEquals(5_007L, clock.nowUnix())

        clock.sync(5_100L)
        assertEquals(5_100L, clock.nowUnix())
        time.elapsedNanos += 1_000_000_000L
        assertEquals(5_101L, clock.nowUnix())
    }

    @Test
    fun ignoresInvalidServerTimestamp() {
        val time = FakeTime()
        val clock = ServerClock(time)
        clock.sync(0L)
        assertFalse(clock.isSynced)
        assertEquals(5_000L, clock.nowUnix())
    }

    @Test
    fun computesTimerRemainingFromMonotonicServerTime() {
        val time = FakeTime()
        val clock = ServerClock(time)
        clock.sync(1_000L)

        val running = Timer(
            id = "t1",
            label = "Tea",
            durationSeconds = 300,
            remainingSeconds = 0,
            dueUnix = 1_200L,
            state = TimerState.RUNNING,
            revision = 1,
            createdUnix = 0,
            updatedUnix = 0,
        )
        assertEquals(200L, clock.remainingSeconds(running))

        time.wallMillis += 3_600_000L
        assertEquals(200L, clock.remainingSeconds(running))

        time.elapsedNanos += 250_000_000_000L
        assertEquals(0L, clock.remainingSeconds(running))

        val paused = running.copy(state = TimerState.PAUSED, remainingSeconds = 42, dueUnix = 0)
        assertEquals(42L, clock.remainingSeconds(paused))
        assertEquals(0L, clock.remainingSeconds(running.copy(state = TimerState.CANCELLED)))
    }
}
