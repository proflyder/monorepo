package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.time.TimeText
import dev.proflyder.home.core.time.Weekdays
import java.time.LocalDate
import java.time.ZoneId

/** Android-free alarm input rules, unit-tested next to the feature API. */
object AlarmValidation {
    val SUPPORTED_REPEATS = setOf(
        RepeatKind.ONCE,
        RepeatKind.DAILY,
        RepeatKind.WEEKDAYS,
        RepeatKind.WEEKENDS,
        RepeatKind.CUSTOM,
    )

    fun validate(draft: AlarmDraft): RemoteFailure? {
        if (draft.label.trim().isEmpty()) return RemoteFailure.Validation(Field.LABEL)
        if (draft.label.trim().length > 120) return RemoteFailure.Validation(Field.LABEL)
        if (draft.hour !in 0..23 || draft.minute !in 0..59) {
            return RemoteFailure.Validation(Field.TIME)
        }
        if (draft.repeat !in SUPPORTED_REPEATS) return RemoteFailure.Validation(Field.REPEAT)
        if (draft.repeat == RepeatKind.CUSTOM && !Weekdays.isValid(draft.weekdays)) {
            return RemoteFailure.Validation(Field.WEEKDAYS)
        }
        val zone = when (val parsed = TimeText.parseZone(draft.timezone)) {
            is TimeText.ParseResult.Valid -> parsed.value
            is TimeText.ParseResult.Invalid -> return RemoteFailure.Validation(Field.TIMEZONE)
        }
        if (draft.repeat == RepeatKind.ONCE) {
            val date = draft.onceDate ?: return RemoteFailure.Validation(Field.DATE)
            if (date.isBefore(LocalDate.now(ZoneId.of(zone.id)))) {
                return RemoteFailure.Validation(Field.DATE)
            }
        }
        return null
    }
}
