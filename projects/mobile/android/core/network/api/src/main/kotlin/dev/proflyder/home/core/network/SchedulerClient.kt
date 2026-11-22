package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.Timer

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

    suspend fun skipScheduleOccurrence(id: String): Outcome<Schedule>

    suspend fun startTimer(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer>
    suspend fun listTimers(includeFinished: Boolean): Outcome<List<Timer>>
    suspend fun pauseTimer(id: String): Outcome<Timer>
    suspend fun resumeTimer(id: String): Outcome<Timer>
    suspend fun cancelTimer(id: String): Outcome<Timer>

    suspend fun enqueueTask(
        taskType: String,
        queue: String,
        payloadJson: String,
        idempotencyKey: String,
    ): Outcome<String>
}
