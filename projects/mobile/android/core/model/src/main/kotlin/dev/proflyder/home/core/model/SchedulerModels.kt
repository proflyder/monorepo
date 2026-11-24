package dev.proflyder.home.core.model

enum class ScheduleKind {
    UNSPECIFIED,
    ALARM,
    REMINDER,
    SCENARIO,
}

enum class RepeatKind {
    UNSPECIFIED,
    ONCE,
    DAILY,
    WEEKDAYS,
    WEEKENDS,
    CUSTOM,
}

enum class TimerState {
    UNSPECIFIED,
    RUNNING,
    PAUSED,
    FIRED,
    CANCELLED,
}

data class WallTime(
    val hour: Int,
    val minute: Int,
    val second: Int = 0,
) {
    init {
        require(hour in 0..23) { "hour out of range" }
        require(minute in 0..59) { "minute out of range" }
        require(second in 0..59) { "second out of range" }
    }
}

data class Schedule(
    val id: String,
    val label: String,
    val kind: ScheduleKind,
    val at: WallTime,
    val onceDate: String,
    val repeat: RepeatKind,
    val weekdays: List<Int>,
    val nextDueUnix: Long,
    val enabled: Boolean,
    val snoozeUntilUnix: Long,
    val lastFiredUnix: Long,
    val snoozeCount: Long,
    val revision: Long,
    val createdUnix: Long,
    val updatedUnix: Long,
    val payloadJson: String = "",
    val activeOccurrenceUnix: Long = 0,
    val activeUntilUnix: Long = 0,
) {
    val isSnoozed: Boolean get() = snoozeUntilUnix > 0
}

data class Timer(
    val id: String,
    val label: String,
    val durationSeconds: Long,
    val remainingSeconds: Long,
    val dueUnix: Long,
    val state: TimerState,
    val revision: Long,
    val createdUnix: Long,
    val updatedUnix: Long,
) {
    val isActive: Boolean
        get() = state == TimerState.RUNNING || state == TimerState.PAUSED
}

data class ConsumerStatus(
    val consumerId: String,
    val lastSeenUnix: Long,
    val lastAckUnix: Long,
    val alive: Boolean,
)

data class SchedulerStatus(
    val version: String,
    val schemaVersion: Int,
    val nowUnix: Long,
    val lastTickUnix: Long,
    val pendingEvents: Int,
    val leasedEvents: Int,
    val enabledSchedules: Int,
    val runningTimers: Int,
    val consumers: List<ConsumerStatus>,
    val scenarioExecutionSupported: Boolean,
)

data class ScheduleDraft(
    val label: String,
    val kind: ScheduleKind,
    val at: WallTime,
    val onceDate: String = "",
    val repeat: RepeatKind,
    val weekdays: List<Int> = emptyList(),
    val enabled: Boolean = true,
    val payloadJson: String = "",
    val idempotencyKey: String = "",
)

enum class Field {
    ENDPOINT,
    CONTROL_ENDPOINT,
    INTERCOM_ENDPOINT,
    SECRETS_ENDPOINT,
    TOKEN,
    TIMEOUT,
    LABEL,
    TIME,
    DATE,
    TIMEZONE,
    WEEKDAYS,
    REPEAT,
    DURATION,
    DESCRIPTION,
    TRIGGERS,
    CONDITIONS,
    STEPS,
    SPEAK,
    ENTITY,
    UNSUPPORTED,
}
