package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import org.junit.Assert.assertEquals
import org.junit.Test

class AlarmCardModeTest {
    private val now = 1_800_000_000L

    @Test
    fun idleAlarmHasNoActions() {
        assertEquals(AlarmCardMode.IDLE, alarmCardMode(alarm(), now))
    }

    @Test
    fun liveOccurrenceHasActions() {
        assertEquals(
            AlarmCardMode.ACTIVE,
            alarmCardMode(
                alarm(activeOccurrence = now, activeUntil = now + 60),
                now,
            ),
        )
    }

    @Test
    fun expiredOccurrenceReturnsToIdle() {
        assertEquals(
            AlarmCardMode.IDLE,
            alarmCardMode(
                alarm(activeOccurrence = now, activeUntil = now - 1),
                now,
            ),
        )
    }

    @Test
    fun snoozedAlarmKeepsActions() {
        assertEquals(
            AlarmCardMode.ACTIVE,
            alarmCardMode(alarm(snoozeUntil = now + 300), now),
        )
    }

    @Test
    fun openEndedOccurrenceHasActions() {
        assertEquals(
            AlarmCardMode.ACTIVE,
            alarmCardMode(alarm(activeOccurrence = now, activeUntil = 0), now),
        )
    }

    private fun alarm(
        activeOccurrence: Long = 0,
        activeUntil: Long = 0,
        snoozeUntil: Long = 0,
    ): Schedule = Schedule(
        id = "alarm-1",
        label = "Work",
        kind = ScheduleKind.ALARM,
        timezone = "UTC",
        at = WallTime(10, 0),
        onceDate = "",
        repeat = RepeatKind.WEEKDAYS,
        weekdays = listOf(1, 2, 3, 4, 5),
        nextDueUnix = now + 3600,
        enabled = true,
        snoozeUntilUnix = snoozeUntil,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        activeOccurrenceUnix = activeOccurrence,
        activeUntilUnix = activeUntil,
    )
}
