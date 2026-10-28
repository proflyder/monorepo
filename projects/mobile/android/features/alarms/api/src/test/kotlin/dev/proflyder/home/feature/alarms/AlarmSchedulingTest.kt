package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AlarmSchedulingTest {
    @Test
    fun oneShotDraftMapsOnceDateAndPreservesRepeat() {
        val date = LocalDate.of(2026, 12, 24)
        val draft = AlarmDraft(
            label = "  Christmas  ",
            hour = 8,
            minute = 5,
            timezone = "Europe/Berlin",
            repeat = RepeatKind.ONCE,
            weekdays = listOf(1, 2),
            enabled = true,
            onceDate = date,
        )
        val wire = draft.toScheduleDraft("key-1")
        assertEquals("Christmas", wire.label)
        assertEquals(ScheduleKind.ALARM, wire.kind)
        assertEquals(RepeatKind.ONCE, wire.repeat)
        assertEquals("2026-12-24", wire.onceDate)
        assertTrue(wire.weekdays.isEmpty())
        assertEquals(WallTime(8, 5), wire.at)
        assertEquals("key-1", wire.idempotencyKey)
    }

    @Test
    fun recurringDraftClearsOneShotDate() {
        val draft = AlarmDraft(
            label = "Daily",
            hour = 7,
            minute = 0,
            timezone = "UTC",
            repeat = RepeatKind.DAILY,
            weekdays = listOf(3),
            enabled = true,
            onceDate = LocalDate.of(2026, 12, 24),
        )
        assertEquals("", draft.toScheduleDraft("k").onceDate)
        assertTrue(draft.toScheduleDraft("k").weekdays.isEmpty())
    }

    @Test
    fun updatePreservesIdRevisionButChangesOneShotDate() {
        val existing = Schedule(
            id = "s1",
            label = "Old",
            kind = ScheduleKind.ALARM,
            timezone = "UTC",
            at = WallTime(6, 0),
            onceDate = "",
            repeat = RepeatKind.DAILY,
            weekdays = emptyList(),
            nextDueUnix = 100,
            enabled = true,
            snoozeUntilUnix = 0,
            lastFiredUnix = 0,
            snoozeCount = 0,
            revision = 9,
            createdUnix = 1,
            updatedUnix = 2,
        )
        val updated = existing.withAlarmDraft(
            AlarmDraft(
                label = "New",
                hour = 9,
                minute = 30,
                timezone = "Asia/Almaty",
                repeat = RepeatKind.ONCE,
                weekdays = emptyList(),
                enabled = false,
                onceDate = LocalDate.of(2027, 1, 1),
            ),
        )
        assertEquals("s1", updated.id)
        assertEquals(9, updated.revision)
        assertEquals(RepeatKind.ONCE, updated.repeat)
        assertEquals("2027-01-01", updated.onceDate)
        assertEquals(WallTime(9, 30), updated.at)
        assertEquals(false, updated.enabled)
    }
}
