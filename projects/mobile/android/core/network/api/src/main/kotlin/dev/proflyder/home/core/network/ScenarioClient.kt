package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft

interface ScenarioClient {
    suspend fun listScenarios(includeDisabled: Boolean): Outcome<List<Scenario>>
    suspend fun getScenario(id: String): Outcome<Scenario>
    suspend fun createScenario(draft: ScenarioDraft): Outcome<Scenario>
    suspend fun updateScenario(scenario: Scenario, expectedRevision: Long): Outcome<Scenario>
    suspend fun deleteScenario(id: String): Outcome<Boolean>
    suspend fun setScenarioEnabled(id: String, enabled: Boolean): Outcome<Scenario>

    suspend fun runScenario(id: String, idempotencyKey: String): Outcome<String>
}
