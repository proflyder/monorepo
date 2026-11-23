package dev.proflyder.home.feature.timers.data

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.data.EntityStore
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.feature.timers.TimerOrder
import dev.proflyder.home.feature.timers.TimerRepository
import dev.proflyder.home.feature.timers.TimersScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(TimersScope::class)
internal class StoreTimerRepository private constructor(
    private val remote: TimerRemote,
    private val store: EntityStore<String, Timer>,
) : TimerRepository, EntitySource<String, Timer> by store {
    @Inject
    constructor(remote: TimerRemote, lifetime: FeatureLifetime) :
        this(remote, EntityStore(lifetime.scope, Timer::id, TimerOrder, remote::list))

    init {
        store.refresh(RefreshMode.SOFT)
    }

    override suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> =
        store.commit({ remote.start(label, durationSeconds, idempotencyKey) }) { upsert(it) }

    override suspend fun pause(id: String): Outcome<Timer> = store.commit({ remote.pause(id) }) { upsert(it) }

    override suspend fun resume(id: String): Outcome<Timer> = store.commit({ remote.resume(id) }) { upsert(it) }

    override suspend fun cancel(id: String): Outcome<Timer> =
        store.optimistic({ it - id }, { remote.cancel(id) }) { remove(id) }
}
