package dev.proflyder.home.feature.scenarios.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.network.ScenarioClient
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/**
 * Remote-only repository. There is no offline copy: the scheduler owns the
 * definition and the phone only edits it.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectScenarioRepository(
    private val client: ScenarioClient,
) : ScenarioRepository {

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
