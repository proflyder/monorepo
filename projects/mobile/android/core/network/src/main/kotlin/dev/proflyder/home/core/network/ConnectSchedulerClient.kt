package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Interceptor
import okhttp3.OkHttpClient

/** Transport-level contract used by feature repositories. */
interface SchedulerClient {
    suspend fun getStatus(): Outcome<SchedulerStatus>

    suspend fun listSchedules(includeDisabled: Boolean, kind: ScheduleKind?): Outcome<List<Schedule>>
    suspend fun getSchedule(id: String): Outcome<Schedule>
    suspend fun createSchedule(draft: ScheduleDraft): Outcome<Schedule>
    suspend fun updateSchedule(schedule: Schedule, expectedRevision: Long): Outcome<Schedule>
    suspend fun deleteSchedule(id: String): Outcome<Boolean>
    suspend fun setScheduleEnabled(id: String, enabled: Boolean): Outcome<Schedule>
    suspend fun snoozeSchedule(id: String, minutes: Int): Outcome<Schedule>
    suspend fun stopSchedule(id: String): Outcome<Schedule>

    /**
     * Advances a recurring schedule past its next occurrence without disabling
     * it. One-shot schedules are rejected by the server; disable them instead.
     */
    suspend fun skipScheduleOccurrence(id: String): Outcome<Schedule>

    suspend fun startTimer(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>>
    suspend fun pauseTimer(id: String): Outcome<Timer>
    suspend fun resumeTimer(id: String): Outcome<Timer>
    suspend fun cancelTimer(id: String): Outcome<Timer>

    /**
     * Durably queues a generic task for an executor group. Returns the event id
     * once the scheduler accepted it; this is "queued", not "executed".
     */
    suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String>
}

/**
 * Connect unary JSON/HTTP client for home.v1.SchedulerService.
 *
 * Security properties:
 *  - redirects (HTTP and HTTPS) are disabled; a bearer token can never be
 *    forwarded to a Location host,
 *  - the token travels only in the Authorization header, never in the URL,
 *  - the system TLS trust store is used; there is no permissive trust manager,
 *  - no socket is bound to Wi-Fi, so LAN/VPN system routing works unchanged.
 */
class ConnectSchedulerClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val serverClock: ServerClock,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    /** Extra application interceptors (logging, inspection); they run after the connection interceptor. */
    interceptors: List<Interceptor> = emptyList(),
) : SchedulerClient {

    private val api: SchedulerApi = HomeRetrofit.create(
        SchedulerApi::class.java,
        settings,
        baseClient,
        Target.SCHEDULER,
        interceptors,
    )

    override suspend fun getStatus(): Outcome<SchedulerStatus> = io {
        api.getStatus(EmptyBody).mapResult { status ->
            status.toDomain().also { serverClock.sync(it.nowUnix) }
        }
    }

    override suspend fun listSchedules(includeDisabled: Boolean, kind: ScheduleKind?): Outcome<List<Schedule>> = io {
        val request = ListSchedulesRequestDto(
            includeDisabled = includeDisabled,
            kind = kind?.takeIf { it != ScheduleKind.UNSPECIFIED }?.dto(),
        )
        api.listSchedules(request).mapResult {
            requireNotNull(it.schedules) { "schedules must be an array" }
                .map(ScheduleDto::toDomain)
        }
    }

    override suspend fun getSchedule(id: String): Outcome<Schedule> = io {
        api.getSchedule(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }
    }

    override suspend fun createSchedule(draft: ScheduleDraft): Outcome<Schedule> = io {
        api.createSchedule(SchedulerJson.createScheduleRequest(draft))
            .mapResult { it.schedule.toDomain() }
    }

    override suspend fun updateSchedule(schedule: Schedule, expectedRevision: Long): Outcome<Schedule> = io {
        api.updateSchedule(SchedulerJson.updateScheduleRequest(schedule, expectedRevision))
            .mapResult { it.schedule.toDomain() }
    }

    override suspend fun deleteSchedule(id: String): Outcome<Boolean> = io {
        api.deleteSchedule(ScheduleIdRequestDto(id)).mapResult { it.deleted }
    }

    override suspend fun setScheduleEnabled(id: String, enabled: Boolean): Outcome<Schedule> = io {
        api.setScheduleEnabled(SetScheduleEnabledRequestDto(id, enabled))
            .mapResult { it.schedule.toDomain() }
    }

    override suspend fun snoozeSchedule(id: String, minutes: Int): Outcome<Schedule> = io {
        api.snoozeSchedule(SnoozeScheduleRequestDto(id, minutes))
            .mapResult { it.schedule.toDomain() }
    }

    override suspend fun stopSchedule(id: String): Outcome<Schedule> = io {
        api.stopSchedule(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }
    }

    override suspend fun skipScheduleOccurrence(id: String): Outcome<Schedule> = io {
        api.skipScheduleOccurrence(ScheduleIdRequestDto(id)).mapResult { it.schedule.toDomain() }
    }

    override suspend fun startTimer(
        label: String,
        durationSeconds: Long,
        idempotencyKey: String,
    ): Outcome<Timer> = io {
        val request = StartTimerRequestDto(
            label = label,
            durationSeconds = durationSeconds,
            idempotencyKey = idempotencyKey.takeIf { it.isNotBlank() },
        )
        api.startTimer(request).mapResult { it.timer.toDomain() }
    }

    override suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>> = io {
        api.listTimers(ListTimersRequestDto(includeFinished)).mapResult {
            requireNotNull(it.timers) { "timers must be an array" }
                .map(TimerDto::toDomain)
        }
    }

    override suspend fun pauseTimer(id: String): Outcome<Timer> = io {
        api.pauseTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }
    }

    override suspend fun resumeTimer(id: String): Outcome<Timer> = io {
        api.resumeTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }
    }

    override suspend fun cancelTimer(id: String): Outcome<Timer> = io {
        api.cancelTimer(ScheduleIdRequestDto(id)).mapResult { it.timer.toDomain() }
    }

    override suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String> = io {
        val request = EnqueueTaskRequestDto(
            taskType = taskType,
            payloadJson = payloadJson,
            queue = queue,
            idempotencyKey = idempotencyKey.takeIf { it.isNotBlank() },
        )
        api.enqueueTask(request).mapResult { it.event.id }
    }

    private suspend fun <T> io(block: () -> Outcome<T>): Outcome<T> = withContext(ioDispatcher) { block() }
}
