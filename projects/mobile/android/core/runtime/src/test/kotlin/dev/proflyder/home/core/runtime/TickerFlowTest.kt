package dev.proflyder.home.core.runtime

import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TickerFlowTest {
    @Test
    fun ticksImmediatelyAndThenEveryPeriod() = runTest {
        var ticks = 0
        val job = tickerFlow(1.seconds).onEach { ticks += 1 }.launchIn(this)
        runCurrent()
        assertEquals(1, ticks)
        advanceTimeBy(3.seconds)
        runCurrent()
        assertEquals(4, ticks)
        job.cancel()
    }

    @Test
    fun theInitialDelayPostponesTheFirstTick() = runTest {
        var ticks = 0
        val job = tickerFlow(1.seconds, initialDelay = 2.seconds).onEach { ticks += 1 }.launchIn(this)
        advanceTimeBy(1.seconds)
        runCurrent()
        assertEquals(0, ticks)
        advanceTimeBy(1.seconds)
        runCurrent()
        assertEquals(1, ticks)
        job.cancel()
    }

    @Test(expected = IllegalArgumentException::class)
    fun aZeroPeriodIsRejected() {
        tickerFlow(0.seconds)
    }
}
