package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Interceptor
import okhttp3.OkHttpClient

/** Transport-level contract used by the scenario feature repository. */
interface ScenarioClient {
    suspend fun listScenarios(includeDisabled: Boolean): Outcome<List<Scenario>>
    suspend fun getScenario(id: String): Outcome<Scenario>
    suspend fun createScenario(draft: ScenarioDraft): Outcome<Scenario>
    suspend fun updateScenario(scenario: Scenario, expectedRevision: Long): Outcome<Scenario>
    suspend fun deleteScenario(id: String): Outcome<Boolean>
    suspend fun setScenarioEnabled(id: String, enabled: Boolean): Outcome<Scenario>

    /**
     * Queues one manual run. Returns the durable `scenario.fire` event id once
     * the scheduler accepted it; this is "queued", not "executed".
     */
    suspend fun runScenario(id: String, idempotencyKey: String): Outcome<String>
}

/**
 * Connect unary JSON/HTTP client for home.v1.ScenarioService.
 *
 * It uses the scheduler endpoint and bearer token, with the same security
 * properties as [ConnectSchedulerClient]: redirects are refused (the token must
 * never follow a Location), the token travels only in the Authorization header,
 * and the system TLS trust store is used.
 */
class ConnectScenarioClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    /** Extra application interceptors (logging, inspection); they run after the connection interceptor. */
    interceptors: List<Interceptor> = emptyList(),
) : ScenarioClient {

    private val api: ScenarioApi = HomeRetrofit.create(
        ScenarioApi::class.java,
        settings,
        baseClient,
        Target.SCHEDULER,
        interceptors,
    )

    override suspend fun listScenarios(includeDisabled: Boolean): Outcome<List<Scenario>> {
        val request = ListScenariosRequestDto(includeDisabled)
        return io {
            api.listScenarios(request).mapResult {
                requireNotNull(it.scenarios) { "scenarios must be an array" }
                    .map(ScenarioDto::toDomain)
            }
        }
    }

    override suspend fun getScenario(id: String): Outcome<Scenario> = io {
        api.getScenario(ScenarioIdRequestDto(id)).mapResult { it.scenario.toDomain() }
    }

    override suspend fun createScenario(draft: ScenarioDraft): Outcome<Scenario> = io {
        api.createScenario(ScenarioJson.createRequest(draft)).mapResult { it.scenario.toDomain() }
    }

    override suspend fun updateScenario(scenario: Scenario, expectedRevision: Long): Outcome<Scenario> {
        val request = ScenarioJson.updateRequest(scenario, expectedRevision)
        return io { api.updateScenario(request).mapResult { it.scenario.toDomain() } }
    }

    override suspend fun deleteScenario(id: String): Outcome<Boolean> = io {
        api.deleteScenario(ScenarioIdRequestDto(id)).mapResult { it.deleted }
    }

    override suspend fun setScenarioEnabled(id: String, enabled: Boolean): Outcome<Scenario> = io {
        api.setScenarioEnabled(SetScenarioEnabledRequestDto(id, enabled))
            .mapResult { it.scenario.toDomain() }
    }

    override suspend fun runScenario(id: String, idempotencyKey: String): Outcome<String> {
        val request = RunScenarioRequestDto(id, idempotencyKey.takeIf { it.isNotBlank() })
        return io { api.runScenario(request).mapResult { it.event.id } }
    }

    private suspend fun <T> io(block: () -> Outcome<T>): Outcome<T> = withContext(ioDispatcher) { block() }
}
