package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReminderValidationTest {
    private val today: LocalDate = LocalDate.now()

    private fun draft(
        label: String = "Call home",
        date: LocalDate = today,
        hour: Int = 9,
        minute: Int = 0,
    ) = ReminderDraft(label, date, hour, minute, enabled = true)

    private fun fieldOf(draft: ReminderDraft): Field =
        (ReminderValidation.validate(draft) as RemoteFailure.Validation).field

    @Test
    fun acceptsTodayAndFutureDates() {
        assertNull(ReminderValidation.validate(draft(date = today)))
        assertNull(ReminderValidation.validate(draft(date = today.plusDays(30))))
    }

    @Test
    fun rejectsPastDate() {
        assertEquals(Field.DATE, fieldOf(draft(date = today.minusDays(1))))
    }

    @Test
    fun rejectsBlankLabelAndBadTime() {
        assertEquals(Field.LABEL, fieldOf(draft(label = " ")))
        assertEquals(Field.TIME, fieldOf(draft(hour = 24)))
        assertEquals(Field.TIME, fieldOf(draft(minute = -1)))
    }

    @Test
    fun rejectsUnknownTimezone() {
    }
}
