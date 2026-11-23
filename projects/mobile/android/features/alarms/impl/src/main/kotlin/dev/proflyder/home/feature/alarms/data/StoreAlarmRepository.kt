package dev.proflyder.home.feature.alarms.data

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.data.EntityStore
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.map
import dev.proflyder.home.feature.alarms.AlarmDraft
import dev.proflyder.home.feature.alarms.AlarmOrder
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.AlarmsScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AlarmsScope::class)
internal class StoreAlarmRepository private constructor(
    private val remote: AlarmRemote,
    private val store: EntityStore<String, Schedule>,
) : AlarmRepository, EntitySource<String, Schedule> by store {
    @Inject
    constructor(remote: AlarmRemote, lifetime: FeatureLifetime) :
        this(remote, EntityStore(lifetime.scope, Schedule::id, AlarmOrder, remote::list))

    init {
        store.refresh(RefreshMode.SOFT)
    }

    override suspend fun load(id: String): Outcome<Schedule> = replace { remote.get(id) }

    override suspend fun create(draft: AlarmDraft, idempotencyKey: String): Outcome<Schedule> =
        replace { remote.create(draft, idempotencyKey) }

    override suspend fun update(existing: Schedule, draft: AlarmDraft): Outcome<Schedule> =
        replace { remote.update(existing, draft) }

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> = store.optimistic(
        change = { current -> current[id]?.let { current + (id to it.copy(enabled = enabled)) } ?: current },
        request = { remote.setEnabled(id, enabled) },
    ) { upsert(it) }

    override suspend fun skipNextOccurrence(id: String): Outcome<Schedule> = replace { remote.skipNextOccurrence(id) }

    override suspend fun delete(id: String): Outcome<Unit> =
        store.optimistic({ it - id }, { remote.delete(id).map { } }) { remove(id) }

    override suspend fun snooze(id: String, minutes: Int): Outcome<Schedule> = replace { remote.snooze(id, minutes) }

    override suspend fun stop(id: String): Outcome<Schedule> = replace { remote.stop(id) }

    private suspend fun replace(request: suspend () -> Outcome<Schedule>): Outcome<Schedule> =
        store.commit(request) { upsert(it) }
}
