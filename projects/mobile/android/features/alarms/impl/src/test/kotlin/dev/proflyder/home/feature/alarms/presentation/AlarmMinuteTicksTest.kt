package dev.proflyder.home.feature.alarms.presentation

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmMinuteTicksTest {
    @Test
    fun ticksOnEachMinuteBoundary() = runTest {
        val start = 10 * 60_000L + 45_000L
        val ticks = minuteTicks { start + testScheduler.currentTime }.take(3).toList()
        assertEquals(listOf(10L, 11L, 12L), ticks)
        assertEquals(15_000L + 60_000L, testScheduler.currentTime)
    }
}
