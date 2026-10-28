package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import java.time.LocalDate

/** Domain contract for one-shot reminders; independent of Android. */
interface ReminderRepository {
    suspend fun list(): Outcome<List<Schedule>>
    suspend fun get(id: String): Outcome<Schedule>
    suspend fun create(draft: ReminderDraft, idempotencyKey: String): Outcome<Schedule>
    suspend fun update(existing: Schedule, draft: ReminderDraft): Outcome<Schedule>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule>
    suspend fun delete(id: String): Outcome<Boolean>
}

data class ReminderDraft(
    val label: String,
    val date: LocalDate,
    val hour: Int,
    val minute: Int,
    val timezone: String,
    val enabled: Boolean,
)
