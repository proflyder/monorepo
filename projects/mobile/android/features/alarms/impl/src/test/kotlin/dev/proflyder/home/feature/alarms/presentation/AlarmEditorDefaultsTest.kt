package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.feature.alarms.AlarmEditorDraft
import dev.proflyder.home.feature.alarms.AlarmEditorDraftCodec
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Test

/** A brand-new alarm is a one-shot for today; editing an existing alarm is untouched. */
class AlarmEditorDefaultsTest {
    @Test
    fun newAlarmEditorStateDefaultsToSingleOccurrence() {
        val state = AlarmEditorUiState()
        assertEquals(RepeatKind.ONCE, state.repeat)
        assertEquals(LocalDate.now(), state.onceDate)
    }

    @Test
    fun newAlarmDraftDefaultsToSingleOccurrence() {
        val draft = AlarmEditorDraft()
        assertEquals(RepeatKind.ONCE, draft.repeat)
        assertEquals(LocalDate.now(), draft.onceDate)
    }

    @Test
    fun editedAlarmKeepsItsRepeatKind() {
        val existing = AlarmEditorDraft(
            existingId = "s1",
            repeat = RepeatKind.WEEKDAYS,
            weekdays = listOf(1, 2, 3, 4, 5),
            onceDate = null,
        )
        val restored = AlarmEditorDraftCodec.fromMap(AlarmEditorDraftCodec.toMap(existing))!!
        assertEquals(RepeatKind.WEEKDAYS, restored.repeat)
        assertEquals(listOf(1, 2, 3, 4, 5), restored.weekdays)
    }
}
