package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import org.junit.Assert.assertEquals
import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.assertNull
import org.junit.Test

class AlarmValidationTest {
    private fun draft(
        label: String = "Wake up",
        hour: Int = 7,
        minute: Int = 30,
        timezone: String = "Asia/Almaty",
        repeat: RepeatKind = RepeatKind.DAILY,
        weekdays: List<Int> = emptyList(),
        onceDate: LocalDate? = null,
    ) = AlarmDraft(label, hour, minute, timezone, repeat, weekdays, enabled = true, onceDate = onceDate)

    private fun fieldOf(draft: AlarmDraft): Field =
        (AlarmValidation.validate(draft) as RemoteFailure.Validation).field

    @Test
    fun acceptsDailyWeekdayWeekendAndCustomAlarms() {
        assertNull(AlarmValidation.validate(draft()))
        assertNull(AlarmValidation.validate(draft(repeat = RepeatKind.WEEKDAYS)))
        assertNull(AlarmValidation.validate(draft(repeat = RepeatKind.WEEKENDS)))
        assertNull(
            AlarmValidation.validate(draft(repeat = RepeatKind.CUSTOM, weekdays = listOf(1, 3, 5))),
        )
        assertNull(AlarmValidation.validate(draft(label = "  Gym  ")))
    }

    @Test
    fun rejectsBlankAndOverlongLabels() {
        assertEquals(Field.LABEL, fieldOf(draft(label = "   ")))
        assertEquals(Field.LABEL, fieldOf(draft(label = "x".repeat(121))))
    }

    @Test
    fun rejectsInvalidTime() {
        assertEquals(Field.TIME, fieldOf(draft(hour = 24)))
        assertEquals(Field.TIME, fieldOf(draft(minute = 60)))
    }

    @Test
    fun acceptsOneShotWithFutureDateAndPreservesDate() {
        val today = LocalDate.now(ZoneId.of("Asia/Almaty"))
        assertNull(
            AlarmValidation.validate(
                draft(repeat = RepeatKind.ONCE, onceDate = today.plusDays(1)),
            ),
        )
        assertEquals(
            Field.DATE,
            fieldOf(draft(repeat = RepeatKind.ONCE, onceDate = today.minusDays(1))),
        )
        assertEquals(Field.DATE, fieldOf(draft(repeat = RepeatKind.ONCE, onceDate = null)))
    }

    @Test
    fun rejectsUnknownRepeatButSupportsOneShot() {
        assertEquals(Field.REPEAT, fieldOf(draft(repeat = RepeatKind.UNSPECIFIED)))
    }

    @Test
    fun customRepeatRequiresValidWeekdays() {
        assertEquals(Field.WEEKDAYS, fieldOf(draft(repeat = RepeatKind.CUSTOM, weekdays = emptyList())))
        assertEquals(Field.WEEKDAYS, fieldOf(draft(repeat = RepeatKind.CUSTOM, weekdays = listOf(0, 1))))
    }

    @Test
    fun rejectsUnknownTimezone() {
        assertEquals(Field.TIMEZONE, fieldOf(draft(timezone = "Mars/Olympus")))
        assertEquals(Field.TIMEZONE, fieldOf(draft(timezone = "Asia Almaty")))
    }
}
