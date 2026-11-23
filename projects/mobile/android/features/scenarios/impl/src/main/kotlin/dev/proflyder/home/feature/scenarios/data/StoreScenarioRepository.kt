package dev.proflyder.home.feature.scenarios.data

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.data.EntityStore
import dev.proflyder.home.core.data.EntitySync
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.map
import dev.proflyder.home.feature.scenarios.ScenarioOrder
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.scenarios.ScenariosScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(ScenariosScope::class)
internal class StoreScenarioRepository private constructor(
    private val remote: ScenarioRemote,
    private val store: EntityStore<String, Scenario>,
) : ScenarioRepository, EntitySource<String, Scenario> by store {
    @Inject
    constructor(remote: ScenarioRemote, lifetime: FeatureLifetime, signals: SyncSignals) :
        this(remote, EntityStore(lifetime.scope, Scenario::id, ScenarioOrder, remote::list)) {
        EntitySync(store, SyncEntity.SCENARIOS, remote::get).follow(signals, lifetime.scope)
    }

    init {
        store.refresh(RefreshMode.SOFT)
    }

    override suspend fun load(id: String): Outcome<Scenario> = replace { remote.get(id) }

    override suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario> =
        replace { remote.create(draft, idempotencyKey) }

    override suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario> =
        replace { remote.update(existing, draft) }

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario> = store.optimistic(
        change = { current -> current[id]?.let { current + (id to it.copy(enabled = enabled)) } ?: current },
        request = { remote.setEnabled(id, enabled) },
    ) { upsert(it) }

    override suspend fun delete(id: String): Outcome<Unit> =
        store.optimistic({ it - id }, { remote.delete(id).map { } }) { remove(id) }

    override suspend fun run(id: String, idempotencyKey: String): Outcome<String> = remote.run(id, idempotencyKey)

    private suspend fun replace(request: suspend () -> Outcome<Scenario>): Outcome<Scenario> =
        store.commit(request) { upsert(it) }
}
