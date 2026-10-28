package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AlarmEditorDraftCodecTest {
    @Test
    fun roundTripsRecurringDraft() {
        val draft = AlarmEditorDraft(
            existingId = "s1",
            existingRevision = 4,
            label = "Wake",
            hour = 7,
            minute = 30,
            timezone = "Asia/Almaty",
            repeat = RepeatKind.CUSTOM,
            weekdays = listOf(1, 3, 5),
            enabled = false,
        )
        val restored = AlarmEditorDraftCodec.fromMap(AlarmEditorDraftCodec.toMap(draft))
        assertEquals(draft, restored)
    }

    @Test
    fun preservesOneShotDateAndDoesNotBecomeDaily() {
        val date = LocalDate.of(2026, 11, 2)
        val draft = AlarmEditorDraft(
            label = "One shot",
            hour = 6,
            minute = 15,
            repeat = RepeatKind.ONCE,
            onceDate = date,
        )
        val restored = AlarmEditorDraftCodec.fromMap(AlarmEditorDraftCodec.toMap(draft))!!
        assertEquals(RepeatKind.ONCE, restored.repeat)
        assertEquals(date, restored.onceDate)
    }

    @Test
    fun malformedOrEmptyDraftIsRejected() {
        assertNull(AlarmEditorDraftCodec.fromMap(emptyMap()))
        assertNull(
            AlarmEditorDraftCodec.fromMap(
                mapOf("alarm_hour" to 25, "alarm_minute" to 0, "alarm_timezone" to "UTC", "alarm_repeat" to "DAILY"),
            ),
        )
        assertNull(
            AlarmEditorDraftCodec.fromMap(
                mapOf("alarm_hour" to 7, "alarm_minute" to 0, "alarm_timezone" to "UTC", "alarm_repeat" to "BOGUS"),
            ),
        )
        // Invalid saved weekdays are filtered rather than crashing.
        val restored = AlarmEditorDraftCodec.fromMap(
            mapOf(
                "alarm_hour" to 7,
                "alarm_minute" to 0,
                "alarm_timezone" to "UTC",
                "alarm_repeat" to "CUSTOM",
                "alarm_weekdays" to arrayListOf(0, 2, 9),
            ),
        )!!
        assertEquals(listOf(2), restored.weekdays)
    }
}
