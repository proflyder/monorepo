package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import java.time.LocalDate

/** Domain contract for alarms; independent of Android and networking. */
interface AlarmRepository {
    suspend fun list(): Outcome<List<Schedule>>
    suspend fun get(id: String): Outcome<Schedule>
    suspend fun create(draft: AlarmDraft, idempotencyKey: String): Outcome<Schedule>
    suspend fun update(existing: Schedule, draft: AlarmDraft): Outcome<Schedule>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule>
    suspend fun delete(id: String): Outcome<Boolean>
    suspend fun snooze(id: String, minutes: Int): Outcome<Schedule>
    suspend fun stop(id: String): Outcome<Schedule>
}

data class AlarmDraft(
    val label: String,
    val hour: Int,
    val minute: Int,
    val timezone: String,
    val repeat: RepeatKind,
    val weekdays: List<Int>,
    val enabled: Boolean,
    /** Required when [repeat] is ONCE; ignored for recurring alarms. */
    val onceDate: LocalDate? = null,
)
