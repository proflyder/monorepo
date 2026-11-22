package dev.proflyder.home.feature.alarms.domain

import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.AlarmValidation
import dev.proflyder.home.feature.alarms.withDefaultLabel
import dev.zacsweers.metro.Inject

@Inject class SaveAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(
        existing: Schedule?,
        draft: AlarmDraft,
        idempotencyKey: String,
        defaultLabel: String,
    ): Outcome<Schedule> {
        val resolved = draft.withDefaultLabel(defaultLabel)
        AlarmValidation.validate(resolved)?.let { return Outcome.Failure(it) }
        return if (existing == null) {
            repository.create(resolved, idempotencyKey)
        } else {
            repository.update(existing, resolved)
        }
    }
}

@Inject class SnoozeAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String, minutes: Int = DEFAULT_MINUTES): Outcome<Schedule> {
        if (minutes !in 1..MAX_MINUTES) return Outcome.Failure(RemoteFailure.Validation(Field.TIME))
        return repository.snooze(id, minutes)
    }

    companion object {
        const val DEFAULT_MINUTES = 9
        private const val MAX_MINUTES = 1440
    }
}
