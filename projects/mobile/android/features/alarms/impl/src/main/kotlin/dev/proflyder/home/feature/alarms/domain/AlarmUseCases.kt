package dev.proflyder.home.feature.alarms.domain

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.AlarmValidation
import dev.zacsweers.metro.Inject

@Inject class LoadAlarms(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(): Outcome<List<Schedule>> = repository.list()
}

@Inject class LoadAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Schedule> = repository.get(id)
}

@Inject class SaveAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(
        existing: Schedule?,
        draft: AlarmDraft,
        idempotencyKey: String,
    ): Outcome<Schedule> {
        AlarmValidation.validate(draft)?.let { return Outcome.Failure(it) }
        return if (existing == null) repository.create(draft, idempotencyKey) else repository.update(existing, draft)
    }
}

@Inject class SetAlarmEnabled(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String, enabled: Boolean): Outcome<Schedule> =
        repository.setEnabled(id, enabled)
}

@Inject class DeleteAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Boolean> = repository.delete(id)
}

@Inject class SnoozeAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String, minutes: Int = DEFAULT_MINUTES): Outcome<Schedule> {
        if (minutes !in 1..1440) return Outcome.Failure(
            dev.proflyder.home.core.model.RemoteFailure.Validation(dev.proflyder.home.core.model.Field.TIME),
        )
        return repository.snooze(id, minutes)
    }

    companion object {
        const val DEFAULT_MINUTES = 9
    }
}

@Inject class StopAlarm(
    private val repository: AlarmRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Schedule> = repository.stop(id)
}
