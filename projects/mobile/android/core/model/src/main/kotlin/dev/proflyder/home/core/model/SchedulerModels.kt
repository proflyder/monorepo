package dev.proflyder.home.core.model

/** Domain models mirroring the frozen ricardo.v1 scheduler contract. */
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
    val timezone: String,
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

/** Input for CreateSchedule/UpdateSchedule. */
data class ScheduleDraft(
    val label: String,
    val kind: ScheduleKind,
    val timezone: String,
    val at: WallTime,
    val onceDate: String = "",
    val repeat: RepeatKind,
    val weekdays: List<Int> = emptyList(),
    val enabled: Boolean = true,
    val payloadJson: String = "",
    val idempotencyKey: String = "",
)

/**
 * Expected failures are values, not thrown exceptions, so the UI can present
 * honest offline/auth/storage states without parsing exception messages.
 */
sealed interface RemoteFailure {
    data object NotConfigured : RemoteFailure
    data object Unauthorized : RemoteFailure
    data class Offline(val detail: String) : RemoteFailure
    data object Timeout : RemoteFailure
    data class Tls(val detail: String) : RemoteFailure
    data class Server(val code: String, val message: String) : RemoteFailure
    data class ProtocolFailure(val detail: String) : RemoteFailure
    data class Validation(val field: Field, val detail: String = "") : RemoteFailure
    data class Storage(val detail: String) : RemoteFailure
}

enum class Field {
    ENDPOINT,
    CONTROL_ENDPOINT,
    TOKEN,
    TIMEOUT,
    LABEL,
    TIME,
    DATE,
    TIMEZONE,
    WEEKDAYS,
    REPEAT,
    DURATION,
}

/**
 * True when the request may have reached the server even though no success
 * response was observed. Retrying such a create/start must reuse its
 * idempotency key. Validation/auth/configuration failures were definitely not
 * applied and may start a new operation.
 */
fun RemoteFailure.isAmbiguousRetryable(): Boolean = when (this) {
    is RemoteFailure.Offline,
    RemoteFailure.Timeout,
    is RemoteFailure.Tls,
    is RemoteFailure.Server,
    is RemoteFailure.ProtocolFailure,
    -> true
    RemoteFailure.NotConfigured,
    RemoteFailure.Unauthorized,
    is RemoteFailure.Validation,
    is RemoteFailure.Storage,
    -> false
}

sealed interface Outcome<out T> {
    data class Success<T>(val value: T) : Outcome<T>
    data class Failure(val failure: RemoteFailure) : Outcome<Nothing>

    fun getOrNull(): T? = (this as? Success)?.value

    fun failureOrNull(): RemoteFailure? = (this as? Failure)?.failure
}

inline fun <T, R> Outcome<T>.map(transform: (T) -> R): Outcome<R> = when (this) {
    is Outcome.Success -> Outcome.Success(transform(value))
    is Outcome.Failure -> this
}
