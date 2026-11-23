package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.data.EntitySource
import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft

interface ScenarioRepository : EntitySource<String, Scenario> {
    suspend fun load(id: String): Outcome<Scenario>
    suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario>
    suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario>
    suspend fun delete(id: String): Outcome<Unit>

    suspend fun run(id: String, idempotencyKey: String): Outcome<String>
}

fun RemoteFailure.isScenarioConflict(): Boolean =
    this is RemoteFailure.Server && (code == ConnectCode.ABORTED || code == ConnectCode.FAILED_PRECONDITION)

val ScenarioOrder: Comparator<Scenario> = compareBy({ !it.enabled }, { it.label.lowercase() })
