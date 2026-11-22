package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectSchedulerClient(
    private val http: HomeHttp,
    private val serverClock: ServerClock,
) : SchedulerClient {
    private val api: SchedulerApi by lazy { http.rest(SchedulerApi::class, Target.Scheduler) }

    override suspend fun getStatus(): Outcome<SchedulerStatus> =
        api.getStatus(EmptyBody).mapResult { status -> status.toDomain().also { serverClock.sync(it.nowUnix) } }

    override suspend fun listSchedules(includeDisabled: Boolean, kind: ScheduleKind?): Outcome<List<Schedule>> {
        val request = ListSchedulesRequestDto(
            includeDisabled = includeDisabled,
            kind = kind?.takeIf { it != ScheduleKind.UNSPECIFIED }?.dto(),
        )
        return api.listSchedules(request).mapResult {
            requireNotNull(it.schedules) { "schedules must be an array" }.map(ScheduleDto::toDomain)
        }
    }

    override suspend fun getSchedule(id: String): Outcome<Schedule> =
        api.getSchedule(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }

    override suspend fun createSchedule(draft: ScheduleDraft): Outcome<Schedule> =
        api.createSchedule(SchedulerJson.createScheduleRequest(draft)).mapResult { it.schedule.toDomain() }

    override suspend fun updateSchedule(schedule: Schedule, expectedRevision: Long): Outcome<Schedule> =
        api.updateSchedule(SchedulerJson.updateScheduleRequest(schedule, expectedRevision))
            .mapResult { it.schedule.toDomain() }

    override suspend fun deleteSchedule(id: String): Outcome<Boolean> =
        api.deleteSchedule(ScheduleIdRequestDto(id)).mapResult { it.deleted }

    override suspend fun setScheduleEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        api.setScheduleEnabled(SetScheduleEnabledRequestDto(id, enabled)).mapResult { it.schedule.toDomain() }

    override suspend fun snoozeSchedule(id: String, minutes: Int): Outcome<Schedule> =
        api.snoozeSchedule(SnoozeScheduleRequestDto(id, minutes)).mapResult { it.schedule.toDomain() }

    override suspend fun stopSchedule(id: String): Outcome<Schedule> =
        api.stopSchedule(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }

    override suspend fun skipScheduleOccurrence(id: String): Outcome<Schedule> =
        api.skipScheduleOccurrence(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }

    override suspend fun startTimer(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> {
        val request = StartTimerRequestDto(
            label = label,
            durationSeconds = durationSeconds,
            idempotencyKey = idempotencyKey.takeIf { it.isNotBlank() },
        )
        return api.startTimer(request).mapResult { it.timer.toDomain() }
    }

    override suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>> =
        api.listTimers(ListTimersRequestDto(includeFinished)).mapResult {
            requireNotNull(it.timers) { "timers must be an array" }.map(TimerDto::toDomain)
        }

    override suspend fun pauseTimer(id: String): Outcome<Timer> =
        api.pauseTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }

    override suspend fun resumeTimer(id: String): Outcome<Timer> =
        api.resumeTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }

    override suspend fun cancelTimer(id: String): Outcome<Timer> =
        api.cancelTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }

    override suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String> {
        val request = EnqueueTaskRequestDto(
            taskType = taskType,
            payloadJson = payloadJson,
            queue = queue,
            idempotencyKey = idempotencyKey.takeIf { it.isNotBlank() },
        )
        return api.enqueueTask(request).mapResult { it.event.id }
    }
}
