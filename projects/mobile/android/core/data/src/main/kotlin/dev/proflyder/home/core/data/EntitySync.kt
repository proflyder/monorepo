package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

typealias FetchOne<V> = suspend (String) -> Outcome<V>

class EntitySync<V : Any>(
    private val store: EntityStore<String, V>,
    private val entity: SyncEntity,
    private val fetchOne: FetchOne<V>? = null,
) {
    fun follow(signals: SyncSignals, scope: CoroutineScope) {
        signals.signals.filter(::concerns).onEach(::apply).launchIn(scope)
    }

    private fun concerns(signal: SyncSignal): Boolean = signal !is SyncSignal.Changed || signal.entity == entity

    private suspend fun apply(signal: SyncSignal) {
        when (signal) {
            SyncSignal.Stale -> store.refresh(RefreshMode.SOFT)
            is SyncSignal.Changed -> changed(signal)
        }
    }

    private suspend fun changed(signal: SyncSignal.Changed) {
        val id = signal.id?.takeIf { it.isNotBlank() }
        val fetch = fetchOne
        when {
            id == null -> store.refresh(RefreshMode.SOFT)
            signal.op == SyncOp.DELETE -> store.remove(id)
            fetch == null -> store.refresh(RefreshMode.SOFT)
            else -> upsert(fetch, id)
        }
    }

    private suspend fun upsert(fetch: FetchOne<V>, id: String) {
        when (val result = fetch(id)) {
            is Outcome.Success -> store.upsert(result.value)
            is Outcome.Failure -> store.refresh(RefreshMode.SOFT)
        }
    }
}
