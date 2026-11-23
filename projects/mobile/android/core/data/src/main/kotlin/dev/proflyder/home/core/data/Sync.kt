package dev.proflyder.home.core.data

import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlinx.coroutines.flow.Flow

enum class SyncEntity(val wire: String) {
    ALARMS("alarms"),
    REMINDERS("reminders"),
    TIMERS("timers"),
    SCENARIOS("scenarios"),
    NOTIFICATIONS("notifications"),
    ;

    companion object {
        fun parse(raw: String?): SyncEntity? = entries.firstOrNull { it.wire == raw?.trim() }
    }
}

enum class SyncOp(val wire: String) {
    UPSERT("upsert"),
    DELETE("delete"),
    ;

    companion object {
        fun parse(raw: String?): SyncOp? = entries.firstOrNull { it.wire == raw?.trim() }
    }
}

sealed interface SyncSignal {
    data class Changed(val entity: SyncEntity, val op: SyncOp, val id: String?) : SyncSignal

    data object Stale : SyncSignal
}

interface SyncSignals {
    val signals: Flow<SyncSignal>
}

fun interface SyncSignalSink {
    fun publish(signal: SyncSignal)
}

class StalenessPolicy(private val threshold: Duration = DEFAULT_THRESHOLD) {
    fun isStale(away: Duration): Boolean = away >= threshold

    companion object {
        val DEFAULT_THRESHOLD: Duration = 5.minutes
    }
}
