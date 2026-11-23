package dev.proflyder.home.feature.scenarios.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.network.ScenarioClient
import dev.zacsweers.metro.Inject

internal interface ScenarioRemote {
    suspend fun list(): Outcome<List<Scenario>>
    suspend fun get(id: String): Outcome<Scenario>
    suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario>
    suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario>
    suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario>
    suspend fun delete(id: String): Outcome<Boolean>
    suspend fun run(id: String, idempotencyKey: String): Outcome<String>
}

@Inject
internal class ConnectScenarioRemote(
    private val client: ScenarioClient,
) : ScenarioRemote {
    override suspend fun list(): Outcome<List<Scenario>> = client.listScenarios(includeDisabled = true)

    override suspend fun get(id: String): Outcome<Scenario> = client.getScenario(id)

    override suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario> =
        client.createScenario(draft.copy(idempotencyKey = idempotencyKey))

    override suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario> =
        client.updateScenario(
            existing.copy(
                label = draft.label,
                description = draft.description,
                enabled = draft.enabled,
                triggers = draft.triggers,
                conditions = draft.conditions,
                steps = draft.steps,
                continueOnError = draft.continueOnError,
            ),
            expectedRevision = existing.revision,
        )

    override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario> =
        client.setScenarioEnabled(id, enabled)

    override suspend fun delete(id: String): Outcome<Boolean> = client.deleteScenario(id)

    override suspend fun run(id: String, idempotencyKey: String): Outcome<String> =
        client.runScenario(id, idempotencyKey)
}
