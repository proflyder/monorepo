package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft

/** Domain contract for scenario definitions; Android- and transport-free. */
interface ScenarioRepository {
    suspend fun list(): Outcome<List<Scenario>>
    suspend fun get(id: String): Outcome<Scenario>
    suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario>
    suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario>
    suspend fun delete(id: String): Outcome<Boolean>

    /** Queues one manual run; returns the durable event id. */
    suspend fun run(id: String, idempotencyKey: String): Outcome<String>
}

/**
 * An UpdateScenario rejected because another client saved a newer revision.
 * The UI must reload instead of overwriting the other change.
 */
fun RemoteFailure.isScenarioConflict(): Boolean =
    this is RemoteFailure.Server && (code == "aborted" || code == "failed_precondition")
