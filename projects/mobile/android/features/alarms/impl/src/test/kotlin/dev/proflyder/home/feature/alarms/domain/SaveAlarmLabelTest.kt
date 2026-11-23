package dev.proflyder.home.feature.alarms.domain

import dev.proflyder.home.core.testing.FakeSyncSignals
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.StorageReason
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.data.AlarmRemote
import dev.proflyder.home.feature.alarms.data.StoreAlarmRepository
import dev.proflyder.home.feature.alarms.presentation.testLifetime
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class SaveAlarmLabelTest {
    private val defaultLabel = "Будильник"

    private fun draft(label: String) = AlarmDraft(
        label = label,
        hour = 7,
        minute = 30,
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        enabled = true,
        onceDate = null,
    )

    private fun existing() = Schedule(
        id = "alarm-1",
        label = "Old",
        kind = ScheduleKind.ALARM,
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
        val repository = RecordingRemote()
        repository.saving()(null, draft(""), "k", defaultLabel)
        repository.saving()(existing(), draft("   "), "", defaultLabel)
        assertEquals(listOf("Будильник"), repository.created)
        assertEquals(listOf("Будильник"), repository.updated)
    }

    @Test
    fun typedLabelIsSentUnchangedOnCreateAndUpdate() = runBlocking {
        val repository = RecordingRemote()
        repository.saving()(null, draft("Gym"), "k", defaultLabel)
        repository.saving()(existing(), draft("Gym"), "", defaultLabel)
        assertEquals(listOf("Gym"), repository.created)
        assertEquals(listOf("Gym"), repository.updated)
    }

    private class RecordingRemote : AlarmRemote {
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

        private fun <T> unused(): Outcome<T> = Outcome.Failure(RemoteFailure.Storage(StorageReason.SETTINGS_WRITE))
    }

    private fun RecordingRemote.saving(): SaveAlarm = SaveAlarm(StoreAlarmRepository(this, testLifetime(), FakeSyncSignals()))
}
