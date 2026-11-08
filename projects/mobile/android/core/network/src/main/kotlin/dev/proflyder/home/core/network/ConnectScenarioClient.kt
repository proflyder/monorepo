package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
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

    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = false
    }

    private val api: ScenarioApi = HomeRetrofit.create(
        ScenarioApi::class.java,
        settings,
        baseClient,
        Target.SCHEDULER,
        interceptors,
    )

    override suspend fun listScenarios(includeDisabled: Boolean): Outcome<List<Scenario>> =
        call("ListScenarios", buildJsonObject { put("includeDisabled", includeDisabled) }) { element ->
            decodeScenarios(element)
        }

    override suspend fun getScenario(id: String): Outcome<Scenario> =
        call("GetScenario", buildJsonObject { put("id", id) }) { element ->
            ScenarioJson.parseScenario(element.jsonObject.getValue("scenario"))
        }

    override suspend fun createScenario(draft: ScenarioDraft): Outcome<Scenario> =
        call("CreateScenario", ScenarioJson.createRequest(draft)) { element ->
            ScenarioJson.parseScenario(element.jsonObject.getValue("scenario"))
        }

    override suspend fun updateScenario(scenario: Scenario, expectedRevision: Long): Outcome<Scenario> =
        call("UpdateScenario", ScenarioJson.updateRequest(scenario, expectedRevision)) { element ->
            ScenarioJson.parseScenario(element.jsonObject.getValue("scenario"))
        }

    override suspend fun deleteScenario(id: String): Outcome<Boolean> =
        call("DeleteScenario", buildJsonObject { put("id", id) }) { element ->
            element.jsonObject["deleted"]?.jsonPrimitive?.content == "true"
        }

    override suspend fun setScenarioEnabled(id: String, enabled: Boolean): Outcome<Scenario> =
        call("SetScenarioEnabled", buildJsonObject { put("id", id); put("enabled", enabled) }) { element ->
            ScenarioJson.parseScenario(element.jsonObject.getValue("scenario"))
        }

    override suspend fun runScenario(id: String, idempotencyKey: String): Outcome<String> =
        call(
            "RunScenario",
            buildJsonObject {
                put("id", id)
                if (idempotencyKey.isNotBlank()) put("idempotencyKey", idempotencyKey)
            },
        ) { element ->
            element.jsonObject.getValue("event").jsonObject.getValue("id").jsonPrimitive.content
        }

    private suspend fun <T> call(
        method: String,
        body: JsonObject,
        decode: (JsonElement) -> T,
    ): Outcome<T> = withContext(ioDispatcher) {
        val response = runCatching { api.call(method, body) }.getOrElse { failure ->
            when (failure) {
                is IOException, is SerializationException ->
                    return@withContext Outcome.Failure(failure.toCallFailure())
                else -> throw failure
            }
        }

        val code = response.code()
        when {
            code in 300..399 -> Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP $code redirect refused"))
            code == 401 || code == 403 -> Outcome.Failure(RemoteFailure.Unauthorized)
            !response.isSuccessful -> {
                val text = runCatching {
                    response.errorBody()?.use { it.readBounded() }.orEmpty()
                }.getOrElse { failure ->
                    if (failure is IOException) {
                        return@withContext Outcome.Failure(failure.toRemoteFailure())
                    }
                    throw failure
                }
                Outcome.Failure(parseConnectError(code, text))
            }
            else -> {
                val element = response.body()
                    ?: return@withContext Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
                runCatching { decode(element) }.fold(
                    onSuccess = { Outcome.Success(it) },
                    onFailure = { failure ->
                        if (failure !is Exception) throw failure
                        Outcome.Failure(
                            RemoteFailure.ProtocolFailure(
                                sanitizeMessage(failure.message) ?: "unexpected response shape",
                            ),
                        )
                    },
                )
            }
        }
    }

    private fun parseConnectError(httpCode: Int, text: String): RemoteFailure {
        val parsed = runCatching {
            if (text.isBlank()) null else json.parseToJsonElement(text).jsonObject
        }.getOrNull()
        val code = sanitizeMessage((parsed?.get("code") as? JsonPrimitive)?.content).orEmpty()
        val message = sanitizeMessage((parsed?.get("message") as? JsonPrimitive)?.content).orEmpty()
        return when (code) {
            "unauthenticated", "permission_denied" -> RemoteFailure.Unauthorized
            "deadline_exceeded", "canceled" -> RemoteFailure.Timeout
            "unavailable" -> RemoteFailure.Offline(message.ifBlank { "server unavailable" })
            else -> RemoteFailure.Server(
                code = code.ifBlank { "http_$httpCode" },
                message = message.ifBlank { "server returned HTTP $httpCode" },
            )
        }
    }

    private fun decodeScenarios(element: JsonElement): List<Scenario> {
        val field = element.jsonObject["scenarios"] ?: return emptyList()
        val array = field as? JsonArray ?: throw MalformedResponse("scenarios must be an array")
        return array.map(ScenarioJson::parseScenario)
    }

    private class MalformedResponse(message: String) : Exception(message)
}
