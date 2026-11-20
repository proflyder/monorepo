package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import java.time.LocalDate

object ReminderValidation {
    fun validate(draft: ReminderDraft): RemoteFailure? {
        if (draft.label.trim().isEmpty()) return RemoteFailure.Validation(Field.LABEL)
        if (draft.label.trim().length > 120) return RemoteFailure.Validation(Field.LABEL)
        if (draft.hour !in 0..23 || draft.minute !in 0..59) {
            return RemoteFailure.Validation(Field.TIME)
        }
        val today = LocalDate.now()
        if (draft.date.isBefore(today)) return RemoteFailure.Validation(Field.DATE)
        return null
    }
}
