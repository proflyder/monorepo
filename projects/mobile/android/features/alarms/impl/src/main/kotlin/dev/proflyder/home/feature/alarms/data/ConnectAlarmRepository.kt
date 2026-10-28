package dev.proflyder.home.feature.alarms.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.toScheduleDraft
import dev.proflyder.home.feature.alarms.withAlarmDraft
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/**
 * Remote-only repository. There is no offline mock: the home server owns alarm
 * state, and a network failure is surfaced as a failure.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectAlarmRepository(
    private val client: SchedulerClient,
) : AlarmRepository {

    override suspend fun list(): Outcome<List<Schedule>> =
        client.listSchedules(includeDisabled = true, kind = ScheduleKind.ALARM)

    override suspend fun get(id: String): Outcome<Schedule> = client.getSchedule(id)

    override suspend fun create(draft: AlarmDraft, idempotencyKey: String): Outcome<Schedule> =
        client.createSchedule(draft.toScheduleDraft(idempotencyKey = idempotencyKey))

    override suspend fun update(existing: Schedule, draft: AlarmDraft): Outcome<Schedule> =
        client.updateSchedule(existing.withAlarmDraft(draft), expectedRevision = existing.revision)

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        client.setScheduleEnabled(id, enabled)

    override suspend fun delete(id: String): Outcome<Boolean> = client.deleteSchedule(id)

    override suspend fun snooze(id: String, minutes: Int): Outcome<Schedule> =
        client.snoozeSchedule(id, minutes)

    override suspend fun stop(id: String): Outcome<Schedule> = client.stopSchedule(id)
}
