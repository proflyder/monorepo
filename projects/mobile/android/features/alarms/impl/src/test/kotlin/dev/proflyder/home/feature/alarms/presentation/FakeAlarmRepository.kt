package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.data.AlarmRemote
import dev.proflyder.home.feature.alarms.data.StoreAlarmRepository
import dev.proflyder.home.feature.alarms.withAlarmDraft
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

internal class FakeAlarmRemote(var alarms: List<Schedule> = emptyList()) : AlarmRemote {
    var listCalls = 0
    var failure: RemoteFailure? = null
    val createKeys = mutableListOf<String>()
    private var created = 0

    override suspend fun list(): Outcome<List<Schedule>> {
        listCalls++
        return failure?.let { Outcome.Failure(it) } ?: Outcome.Success(alarms)
    }

    override suspend fun get(id: String): Outcome<Schedule> = Outcome.Success(alarms.first { it.id == id })

    override suspend fun create(draft: AlarmDraft, idempotencyKey: String): Outcome<Schedule> {
        createKeys += idempotencyKey
        failure?.let { return Outcome.Failure(it) }
        created++
        val alarm = alarms.first().withAlarmDraft(draft).copy(id = "new-$created")
        alarms = alarms + alarm
        return Outcome.Success(alarm)
    }

    override suspend fun update(existing: Schedule, draft: AlarmDraft): Outcome<Schedule> =
        Outcome.Success(existing.copy(label = draft.label))

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> =
        failure?.let { Outcome.Failure(it) } ?: Outcome.Success(alarms.first { it.id == id }.copy(enabled = enabled))

    override suspend fun skipNextOccurrence(id: String): Outcome<Schedule> = get(id)

    override suspend fun delete(id: String): Outcome<Boolean> {
        failure?.let { return Outcome.Failure(it) }
        alarms = alarms.filterNot { it.id == id }
        return Outcome.Success(true)
    }

    override suspend fun snooze(id: String, minutes: Int): Outcome<Schedule> = get(id)

    override suspend fun stop(id: String): Outcome<Schedule> = get(id)
}

internal fun testLifetime(): FeatureLifetime = FeatureLifetime(CoroutineScope(Dispatchers.Unconfined + SupervisorJob()))

internal fun FakeAlarmRemote.repository(): StoreAlarmRepository = StoreAlarmRepository(this, testLifetime())
