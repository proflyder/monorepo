package dev.proflyder.home.feature.alarms.domain

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The server must never see an empty alarm name: the editor field is optional
 * and [SaveAlarm] fills the localized default where the request is formed.
 */
class SaveAlarmLabelTest {
    private val defaultLabel = "Будильник"

    private fun draft(label: String) = AlarmDraft(
        label = label,
        hour = 7,
        minute = 30,
        timezone = "Asia/Almaty",
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        enabled = true,
        onceDate = null,
    )

    private fun existing() = Schedule(
        id = "alarm-1",
        label = "Old",
        kind = ScheduleKind.ALARM,
        timezone = "Asia/Almaty",
        at = WallTime(7, 30),
        onceDate = "",
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        nextDueUnix = 0,
        enabled = true,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
    )

    @Test
    fun emptyAndWhitespaceLabelsAreSentWithTheDefault() = runBlocking {
        val repository = RecordingRepository()
        SaveAlarm(repository)(null, draft(""), "k", defaultLabel)
        SaveAlarm(repository)(existing(), draft("   "), "", defaultLabel)
        assertEquals(listOf("Будильник"), repository.created)
        assertEquals(listOf("Будильник"), repository.updated)
    }

    @Test
    fun typedLabelIsSentUnchangedOnCreateAndUpdate() = runBlocking {
        val repository = RecordingRepository()
        SaveAlarm(repository)(null, draft("Gym"), "k", defaultLabel)
        SaveAlarm(repository)(existing(), draft("Gym"), "", defaultLabel)
        assertEquals(listOf("Gym"), repository.created)
        assertEquals(listOf("Gym"), repository.updated)
    }

    private class RecordingRepository : AlarmRepository {
        val created = mutableListOf<String>()
        val updated = mutableListOf<String>()

        override suspend fun create(draft: AlarmDraft, idempotencyKey: String): Outcome<Schedule> {
            created += draft.label
            return unused()
        }

        override suspend fun update(existing: Schedule, draft: AlarmDraft): Outcome<Schedule> {
            updated += draft.label
            return unused()
        }

        override suspend fun list(): Outcome<List<Schedule>> = unused()
        override suspend fun get(id: String): Outcome<Schedule> = unused()
        override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> = unused()
        override suspend fun skipNextOccurrence(id: String): Outcome<Schedule> = unused()
        override suspend fun delete(id: String): Outcome<Boolean> = unused()
        override suspend fun snooze(id: String, minutes: Int): Outcome<Schedule> = unused()
        override suspend fun stop(id: String): Outcome<Schedule> = unused()

        private fun <T> unused(): Outcome<T> = Outcome.Failure(RemoteFailure.Storage("unused"))
    }
}
