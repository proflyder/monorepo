package dev.proflyder.home.feature.reminders.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.time.LocalDate

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectReminderRepository(
    private val client: SchedulerClient,
) : ReminderRepository {
    override suspend fun list(): Outcome<List<Schedule>> =
        client.listSchedules(includeDisabled = true, kind = ScheduleKind.REMINDER)

    override suspend fun get(id: String): Outcome<Schedule> = client.getSchedule(id)

    override suspend fun create(draft: ReminderDraft, idempotencyKey: String): Outcome<Schedule> =
        client.createSchedule(draft.toScheduleDraft(idempotencyKey))

    override suspend fun update(existing: Schedule, draft: ReminderDraft): Outcome<Schedule> {
        val updated = existing.copy(
            label = draft.label.trim(),
            timezone = draft.timezone.trim(),
            at = WallTime(draft.hour, draft.minute),
            onceDate = draft.date.toString(),
            repeat = RepeatKind.ONCE,
            weekdays = emptyList(),
            enabled = draft.enabled,
        )
        return client.updateSchedule(updated, expectedRevision = existing.revision)
    }

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        client.setScheduleEnabled(id, enabled)

    override suspend fun delete(id: String): Outcome<Boolean> = client.deleteSchedule(id)

    private fun ReminderDraft.toScheduleDraft(idempotencyKey: String): ScheduleDraft = ScheduleDraft(
        label = label.trim(),
        kind = ScheduleKind.REMINDER,
        timezone = timezone.trim(),
        at = WallTime(hour, minute),
        onceDate = date.toString(),
        repeat = RepeatKind.ONCE,
        enabled = enabled,
        idempotencyKey = idempotencyKey,
    )
}
