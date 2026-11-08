package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import java.time.LocalDate
import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Test

class AlarmQuickOffsetStateTest {
    private val now = LocalDateTime.of(2026, 11, 8, 14, 20)

    @Test
    fun quickOffsetReplacesTimeWithNowPlusOffset() {
        val state = AlarmEditorUiState(hour = 6, minute = 15)

        val updated = state.withQuickOffset(now, offsetMinutes = 90)

        assertEquals(15, updated.hour)
        assertEquals(50, updated.minute)
    }

    @Test
    fun repeatedQuickOffsetCountsFromNowNotFromTheCurrentField() {
        val first = AlarmEditorUiState(hour = 6, minute = 15).withQuickOffset(now, offsetMinutes = 15)
        val second = first.withQuickOffset(now, offsetMinutes = 15)

        assertEquals(14, second.hour)
        assertEquals(35, second.minute)
    }

    @Test
    fun quickOffsetKeepsRepeatDaysDateAndOtherFields() {
        val date = LocalDate.of(2026, 11, 20)
        val state = AlarmEditorUiState(
            label = "Gym",
            hour = 6,
            minute = 15,
            repeat = RepeatKind.CUSTOM,
            weekdays = setOf(1, 3),
            onceDate = date,
            enabled = false,
        )

        val updated = state.withQuickOffset(now, offsetMinutes = 30)

        assertEquals("Gym", updated.label)
        assertEquals(RepeatKind.CUSTOM, updated.repeat)
        assertEquals(setOf(1, 3), updated.weekdays)
        assertEquals(date, updated.onceDate)
        assertEquals(false, updated.enabled)
        assertEquals(14, updated.hour)
        assertEquals(50, updated.minute)
    }

    @Test
    fun quickOffsetCrossesMidnight() {
        val lateNow = LocalDateTime.of(2026, 11, 8, 23, 50)

        val updated = AlarmEditorUiState(hour = 7, minute = 0).withQuickOffset(lateNow, offsetMinutes = 30)

        assertEquals(0, updated.hour)
        assertEquals(20, updated.minute)
    }
}
