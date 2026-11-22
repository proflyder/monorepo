package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectScenarioClient(private val http: HomeHttp) : ScenarioClient {
    private val api: ScenarioApi by lazy { http.rest(ScenarioApi::class, Target.Scheduler) }

    override suspend fun listScenarios(includeDisabled: Boolean): Outcome<List<Scenario>> =
        api.listScenarios(ListScenariosRequestDto(includeDisabled)).mapResult {
            requireNotNull(it.scenarios) { "scenarios must be an array" }.map(ScenarioDto::toDomain)
        }

    override suspend fun getScenario(id: String): Outcome<Scenario> =
        api.getScenario(ScenarioIdRequestDto(id)).mapResult { it.scenario.toDomain() }

    override suspend fun createScenario(draft: ScenarioDraft): Outcome<Scenario> =
        api.createScenario(ScenarioJson.createRequest(draft)).mapResult { it.scenario.toDomain() }

    override suspend fun updateScenario(scenario: Scenario, expectedRevision: Long): Outcome<Scenario> =
        api.updateScenario(ScenarioJson.updateRequest(scenario, expectedRevision)).mapResult { it.scenario.toDomain() }

    override suspend fun deleteScenario(id: String): Outcome<Boolean> =
        api.deleteScenario(ScenarioIdRequestDto(id)).mapResult { it.deleted }

    override suspend fun setScenarioEnabled(id: String, enabled: Boolean): Outcome<Scenario> =
        api.setScenarioEnabled(SetScenarioEnabledRequestDto(id, enabled)).mapResult { it.scenario.toDomain() }

    override suspend fun runScenario(id: String, idempotencyKey: String): Outcome<String> =
        api.runScenario(RunScenarioRequestDto(id, idempotencyKey.takeIf { it.isNotBlank() })).mapResult { it.event.id }
}
