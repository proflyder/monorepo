package dev.proflyder.home.feature.reminders

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import java.time.LocalDate

interface ReminderRepository : EntitySource<String, Schedule> {
    suspend fun load(id: String): Outcome<Schedule>
    suspend fun create(draft: ReminderDraft, idempotencyKey: String): Outcome<Schedule>
    suspend fun update(existing: Schedule, draft: ReminderDraft): Outcome<Schedule>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule>
    suspend fun delete(id: String): Outcome<Unit>
}

data class ReminderDraft(
    val label: String,
    val date: LocalDate,
    val hour: Int,
    val minute: Int,
    val enabled: Boolean,
)
