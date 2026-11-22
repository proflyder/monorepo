package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft

interface ScenarioRepository {
    suspend fun list(): Outcome<List<Scenario>>
    suspend fun get(id: String): Outcome<Scenario>
    suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario>
    suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario>
    suspend fun delete(id: String): Outcome<Boolean>

    suspend fun run(id: String, idempotencyKey: String): Outcome<String>
}

fun RemoteFailure.isScenarioConflict(): Boolean =
    this is RemoteFailure.Server && (code == ConnectCode.ABORTED || code == ConnectCode.FAILED_PRECONDITION)
