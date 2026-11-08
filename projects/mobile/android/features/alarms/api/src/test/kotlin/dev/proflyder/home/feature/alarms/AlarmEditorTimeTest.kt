package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.WallTime
import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Test

class AlarmEditorTimeTest {
    private fun at(hour: Int, minute: Int): LocalDateTime = LocalDateTime.of(2026, 11, 8, hour, minute)

    @Test
    fun defaultTimeRoundsUpToTheNextFullHour() {
        assertEquals(WallTime(15, 0), defaultAlarmTime(at(14, 20)))
        assertEquals(WallTime(15, 0), defaultAlarmTime(at(14, 59)))
    }

    @Test
    fun defaultTimeExactlyOnTheHourStaysInTheFuture() {
        // 14:00 must become 15:00, not 14:00 which is already now.
        assertEquals(WallTime(15, 0), defaultAlarmTime(at(14, 0)))
    }

    @Test
    fun defaultTimeWrapsPastMidnight() {
        assertEquals(WallTime(0, 0), defaultAlarmTime(at(23, 30)))
        assertEquals(WallTime(0, 0), defaultAlarmTime(at(23, 0)))
        assertEquals(WallTime(1, 0), defaultAlarmTime(at(0, 0)))
    }

    @Test
    fun quickOffsetsAreTheSingleDeclaredList() {
        assertEquals(listOf(15, 30, 60, 90, 120), ALARM_QUICK_OFFSETS_MINUTES)
    }

    @Test
    fun quickOffsetAddsUnroundedMinutesToNow() {
        val now = at(14, 20)
        assertEquals(WallTime(14, 35), quickOffsetTime(now, 15))
        assertEquals(WallTime(14, 50), quickOffsetTime(now, 30))
        assertEquals(WallTime(15, 20), quickOffsetTime(now, 60))
        assertEquals(WallTime(15, 50), quickOffsetTime(now, 90))
        assertEquals(WallTime(16, 20), quickOffsetTime(now, 120))
    }

    @Test
    fun quickOffsetCrossesMidnight() {
        assertEquals(WallTime(0, 20), quickOffsetTime(at(23, 50), 30))
        assertEquals(WallTime(0, 10), quickOffsetTime(at(23, 40), 30))
    }

    @Test
    fun repeatedQuickOffsetDoesNotAccumulate() {
        val now = at(14, 20)
        val first = quickOffsetTime(now, 15)
        val second = quickOffsetTime(now, 15)
        assertEquals(first, second)
        assertEquals(WallTime(14, 35), second)
    }
}
