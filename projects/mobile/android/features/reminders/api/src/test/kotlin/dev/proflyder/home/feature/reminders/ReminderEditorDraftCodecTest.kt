package dev.proflyder.home.feature.reminders

import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReminderEditorDraftCodecTest {
    @Test
    fun roundTripsDraft() {
        val draft = ReminderEditorDraft(
            existingId = "r1",
            existingRevision = 3,
            label = "Call",
            date = LocalDate.of(2026, 12, 1),
            hour = 18,
            minute = 45,
            timezone = "Europe/Berlin",
            enabled = false,
        )
        assertEquals(draft, ReminderEditorDraftCodec.fromMap(ReminderEditorDraftCodec.toMap(draft)))
    }

    @Test
    fun malformedDraftIsRejected() {
        assertNull(ReminderEditorDraftCodec.fromMap(emptyMap()))
        assertNull(
            ReminderEditorDraftCodec.fromMap(
                mapOf("reminder_hour" to 24, "reminder_minute" to 0, "reminder_timezone" to "UTC", "reminder_date" to "2026-01-01"),
            ),
        )
        assertNull(
            ReminderEditorDraftCodec.fromMap(
                mapOf("reminder_hour" to 9, "reminder_minute" to 0, "reminder_timezone" to "UTC", "reminder_date" to "not-a-date"),
            ),
        )
    }
}
