package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.time.TimeText
import java.time.LocalDate
import java.time.ZoneId

/** Android-free reminder input rules. One-shot reminders need a future date. */
object ReminderValidation {
    fun validate(draft: ReminderDraft): RemoteFailure? {
        if (draft.label.trim().isEmpty()) return RemoteFailure.Validation(Field.LABEL)
        if (draft.label.trim().length > 120) return RemoteFailure.Validation(Field.LABEL)
        if (draft.hour !in 0..23 || draft.minute !in 0..59) {
            return RemoteFailure.Validation(Field.TIME)
        }
        val zone = when (val parsed = TimeText.parseZone(draft.timezone)) {
            is TimeText.ParseResult.Valid -> parsed.value
            is TimeText.ParseResult.Invalid -> return RemoteFailure.Validation(Field.TIMEZONE)
        }
        val today = LocalDate.now(ZoneId.of(zone.id))
        if (draft.date.isBefore(today)) return RemoteFailure.Validation(Field.DATE)
        return null
    }
}
