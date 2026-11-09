package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ConsumerStatus
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.model.WallTime
import kotlinx.serialization.json.JsonElement

/**
 * Maps home.v1 scheduler DTOs to domain models and builds typed requests.
 *
 * Enums are proto names on the wire and int64 values are decimal strings; both
 * are handled by [WireDtos] so mapping stays a pure field translation.
 */
object SchedulerJson {
    const val SERVICE = "home.v1.SchedulerService"

    fun createScheduleRequest(draft: ScheduleDraft): CreateScheduleRequestDto =
        CreateScheduleRequestDto(
            label = draft.label,
            kind = draft.kind.dto(),
            timezone = draft.timezone,
            at = draft.at.dto(),
            onceDate = draft.onceDate.takeIf { it.isNotBlank() },
            repeat = draft.repeat.dto(),
            weekdays = draft.weekdays.sorted().takeIf { it.isNotEmpty() },
            enabled = draft.enabled,
            payloadJson = draft.payloadJson.takeIf { it.isNotBlank() },
            idempotencyKey = draft.idempotencyKey.takeIf { it.isNotBlank() },
        )

    fun updateScheduleRequest(schedule: Schedule, expectedRevision: Long): UpdateScheduleRequestDto =
        UpdateScheduleRequestDto(
            schedule = ScheduleInputDto(
                id = schedule.id,
                label = schedule.label,
                kind = schedule.kind.dto(),
                timezone = schedule.timezone,
                at = schedule.at.dto(),
                onceDate = schedule.onceDate.takeIf { it.isNotBlank() },
                repeat = schedule.repeat.dto(),
                weekdays = schedule.weekdays.sorted().takeIf { it.isNotEmpty() },
                enabled = schedule.enabled,
                payloadJson = schedule.payloadJson.takeIf { it.isNotBlank() },
                revision = schedule.revision,
            ),
            expectedRevision = expectedRevision,
        )

    fun parseSchedule(element: JsonElement): Schedule =
        WireJson.decodeFromJsonElement(ScheduleDto.serializer(), element).toDomain()

    fun parseTimer(element: JsonElement): Timer =
        WireJson.decodeFromJsonElement(TimerDto.serializer(), element).toDomain()

    fun parseStatus(element: JsonElement): SchedulerStatus =
        WireJson.decodeFromJsonElement(SchedulerStatusDto.serializer(), element).toDomain()
}

internal fun ScheduleDto.toDomain(): Schedule = Schedule(
    id = id,
    label = label,
    kind = kind.toDomain(),
    timezone = timezone,
    at = (at ?: WallTimeDto()).toDomain(),
    onceDate = onceDate,
    repeat = repeat.toDomain(),
    weekdays = weekdays,
    nextDueUnix = nextDueUnix,
    enabled = enabled,
    snoozeUntilUnix = snoozeUntilUnix,
    lastFiredUnix = lastFiredUnix,
    snoozeCount = snoozeCount,
    revision = revision,
    createdUnix = createdUnix,
    updatedUnix = updatedUnix,
    payloadJson = payloadJson,
    activeOccurrenceUnix = activeOccurrenceUnix,
    activeUntilUnix = activeUntilUnix,
)

internal fun TimerDto.toDomain(): Timer = Timer(
    id = id,
    label = label,
    durationSeconds = durationSeconds,
    remainingSeconds = remainingSeconds,
    dueUnix = dueUnix,
    state = state.toDomain(),
    revision = revision,
    createdUnix = createdUnix,
    updatedUnix = updatedUnix,
)

internal fun ConsumerStatusDto.toDomain(): ConsumerStatus = ConsumerStatus(
    consumerId = consumerId,
    lastSeenUnix = lastSeenUnix,
    lastAckUnix = lastAckUnix,
    alive = alive,
)

internal fun SchedulerStatusDto.toDomain(): SchedulerStatus = SchedulerStatus(
    version = version,
    schemaVersion = schemaVersion,
    nowUnix = nowUnix,
    lastTickUnix = lastTickUnix,
    pendingEvents = pendingEvents,
    leasedEvents = leasedEvents,
    enabledSchedules = enabledSchedules,
    runningTimers = runningTimers,
    consumers = consumers.map { it.toDomain() },
    scenarioExecutionSupported = scenarioExecutionSupported,
)

internal fun WallTimeDto.toDomain(): WallTime = WallTime(
    hour = hour.takeIf { it in 0..23 } ?: 0,
    minute = minute.takeIf { it in 0..59 } ?: 0,
    second = second.takeIf { it in 0..59 } ?: 0,
)

internal fun WallTime.dto(): WallTimeDto = WallTimeDto(hour = hour, minute = minute, second = second)

internal fun ScheduleKind.dto(): ScheduleKindDto = when (this) {
    ScheduleKind.UNSPECIFIED -> ScheduleKindDto.UNSPECIFIED
    ScheduleKind.ALARM -> ScheduleKindDto.ALARM
    ScheduleKind.REMINDER -> ScheduleKindDto.REMINDER
    ScheduleKind.SCENARIO -> ScheduleKindDto.SCENARIO
}

internal fun ScheduleKindDto.toDomain(): ScheduleKind = when (this) {
    ScheduleKindDto.UNSPECIFIED -> ScheduleKind.UNSPECIFIED
    ScheduleKindDto.ALARM -> ScheduleKind.ALARM
    ScheduleKindDto.REMINDER -> ScheduleKind.REMINDER
    ScheduleKindDto.SCENARIO -> ScheduleKind.SCENARIO
}

internal fun RepeatKind.dto(): RepeatKindDto = when (this) {
    RepeatKind.UNSPECIFIED -> RepeatKindDto.UNSPECIFIED
    RepeatKind.ONCE -> RepeatKindDto.ONCE
    RepeatKind.DAILY -> RepeatKindDto.DAILY
    RepeatKind.WEEKDAYS -> RepeatKindDto.WEEKDAYS
    RepeatKind.WEEKENDS -> RepeatKindDto.WEEKENDS
    RepeatKind.CUSTOM -> RepeatKindDto.CUSTOM
}

internal fun RepeatKindDto.toDomain(): RepeatKind = when (this) {
    RepeatKindDto.UNSPECIFIED -> RepeatKind.UNSPECIFIED
    RepeatKindDto.ONCE -> RepeatKind.ONCE
    RepeatKindDto.DAILY -> RepeatKind.DAILY
    RepeatKindDto.WEEKDAYS -> RepeatKind.WEEKDAYS
    RepeatKindDto.WEEKENDS -> RepeatKind.WEEKENDS
    RepeatKindDto.CUSTOM -> RepeatKind.CUSTOM
}

internal fun TimerStateDto.toDomain(): TimerState = when (this) {
    TimerStateDto.UNSPECIFIED -> TimerState.UNSPECIFIED
    TimerStateDto.RUNNING -> TimerState.RUNNING
    TimerStateDto.PAUSED -> TimerState.PAUSED
    TimerStateDto.FIRED -> TimerState.FIRED
    TimerStateDto.CANCELLED -> TimerState.CANCELLED
}
