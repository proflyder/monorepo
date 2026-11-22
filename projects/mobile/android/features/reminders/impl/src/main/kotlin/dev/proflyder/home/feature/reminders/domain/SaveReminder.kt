package dev.proflyder.home.feature.reminders.domain

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.reminders.ReminderValidation
import dev.zacsweers.metro.Inject

@Inject class SaveReminder(
    private val repository: ReminderRepository,
) {
    suspend operator fun invoke(
        existing: Schedule?,
        draft: ReminderDraft,
        idempotencyKey: String,
    ): Outcome<Schedule> {
        ReminderValidation.validate(draft)?.let { return Outcome.Failure(it) }
        return if (existing == null) repository.create(draft, idempotencyKey) else repository.update(existing, draft)
    }
}
