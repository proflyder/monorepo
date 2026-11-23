package dev.proflyder.home.feature.reminders.data

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.data.EntityStore
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.map
import dev.proflyder.home.feature.reminders.ReminderDraft
import dev.proflyder.home.feature.reminders.ReminderOrder
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.reminders.RemindersScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(RemindersScope::class)
internal class StoreReminderRepository private constructor(
    private val remote: ReminderRemote,
    private val store: EntityStore<String, Schedule>,
) : ReminderRepository, EntitySource<String, Schedule> by store {
    @Inject
    constructor(remote: ReminderRemote, lifetime: FeatureLifetime) :
        this(remote, EntityStore(lifetime.scope, Schedule::id, ReminderOrder, remote::list))

    init {
        store.refresh(RefreshMode.SOFT)
    }

    override suspend fun load(id: String): Outcome<Schedule> = replace { remote.get(id) }

    override suspend fun create(draft: ReminderDraft, idempotencyKey: String): Outcome<Schedule> =
        replace { remote.create(draft, idempotencyKey) }

    override suspend fun update(existing: Schedule, draft: ReminderDraft): Outcome<Schedule> =
        replace { remote.update(existing, draft) }

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Schedule> = store.optimistic(
        change = { current -> current[id]?.let { current + (id to it.copy(enabled = enabled)) } ?: current },
        request = { remote.setEnabled(id, enabled) },
    ) { upsert(it) }

    override suspend fun delete(id: String): Outcome<Unit> =
        store.optimistic({ it - id }, { remote.delete(id).map { } }) { remove(id) }

    private suspend fun replace(request: suspend () -> Outcome<Schedule>): Outcome<Schedule> =
        store.commit(request) { upsert(it) }
}
