package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import org.junit.Assert.assertEquals
import org.junit.Test

class AlarmDisableChoiceTest {
    private fun alarm(repeat: RepeatKind, enabled: Boolean = true) = Schedule(
        id = "alarm-1",
        label = "Wake up",
        kind = ScheduleKind.ALARM,
        timezone = "UTC",
        at = WallTime(7, 30),
        onceDate = "",
        repeat = repeat,
        weekdays = emptyList(),
        nextDueUnix = 1_767_339_000,
        enabled = enabled,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        activeOccurrenceUnix = 0,
        activeUntilUnix = 0,
    )

    @Test
    fun recurringAlarmsAskBeforeTurningOff() {
        assertEquals(AlarmDisableChoice.ASK, alarmDisableChoice(alarm(RepeatKind.DAILY)))
        assertEquals(AlarmDisableChoice.ASK, alarmDisableChoice(alarm(RepeatKind.WEEKDAYS)))
        assertEquals(AlarmDisableChoice.ASK, alarmDisableChoice(alarm(RepeatKind.WEEKENDS)))
        assertEquals(AlarmDisableChoice.ASK, alarmDisableChoice(alarm(RepeatKind.CUSTOM)))
    }

    @Test
    fun oneShotAlarmKeepsTheDirectSwitch() {
        assertEquals(AlarmDisableChoice.DISABLE, alarmDisableChoice(alarm(RepeatKind.ONCE)))
        assertEquals(AlarmDisableChoice.DISABLE, alarmDisableChoice(alarm(RepeatKind.UNSPECIFIED)))
    }
}
