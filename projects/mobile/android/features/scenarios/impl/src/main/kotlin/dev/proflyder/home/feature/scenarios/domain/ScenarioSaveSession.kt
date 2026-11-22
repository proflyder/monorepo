package dev.proflyder.home.feature.scenarios.domain

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.IdempotentRequestKey
import dev.proflyder.home.feature.scenarios.isScenarioConflict

internal data class PendingCreate(val key: String, val endpoint: String)

internal interface PendingCreateStore {
    var pending: PendingCreate?
}

internal class ScenarioSaveSession(
    private val saveScenario: SaveScenario,
    private val settings: ConnectionSettingsStore,
    private val store: PendingCreateStore,
    private val identity: IdempotentRequestKey<ScenarioDraft> = IdempotentRequestKey(),
) {
    private val endpoint: String get() = settings.config.value.endpoint?.baseUrl().orEmpty()

    fun restore(draft: ScenarioDraft) {
        val saved = store.pending ?: return
        if (saved.endpoint == endpoint) identity.adopt(draft, saved.key) else store.pending = null
    }

    fun invalidate() {
        identity.reset()
        store.pending = null
    }

    suspend fun save(existing: Scenario?, draft: ScenarioDraft): Outcome<Scenario> {
        val key = if (existing == null) createKey(draft) else "".also { store.pending = null }
        val result = saveScenario(existing, draft, key)
        when {
            result is Outcome.Success -> succeeded()
            result is Outcome.Failure && !result.failure.isScenarioConflict() && !result.failure.isAmbiguousRetryable() ->
                invalidate()
        }
        return result
    }

    private fun createKey(draft: ScenarioDraft): String =
        identity.keyFor(draft).also { store.pending = PendingCreate(it, endpoint) }

    private fun succeeded() {
        identity.markSucceeded()
        store.pending = null
    }
}
