package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request

/** Reads the controllable scenarios/devices from the home control service. */
interface ControlCatalogClient {
    suspend fun fetch(): Outcome<ControlCatalog>
}

/** Wire format of GET /v1/catalog and of the scheduler task that carries a command. */
object ControlJson {
    /** Scheduler queue and task type the home control executor consumes. */
    const val QUEUE = "control"
    const val TASK_TYPE = "home.control"

    /** Task payload: a catalog id and an action only, never an HA service or entity. */
    fun commandPayload(entryId: String, action: ControlAction): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", action.wire)
        }.toString()

    fun parseCatalog(element: JsonElement): ControlCatalog {
        val root = element.jsonObject
        val array = root["entries"] as? JsonArray ?: throw IllegalArgumentException("entries must be an array")
        return ControlCatalog(
            entries = array.map { parseEntry(it.jsonObject) },
            statesOk = root["statesOk"]?.jsonPrimitive?.booleanOrNull ?: false,
        )
    }

    private fun parseEntry(o: JsonObject): ControlEntry {
        fun text(name: String) = (o[name] as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()
        val kind = when (text("kind")) {
            "scenario" -> ControlKind.SCENARIO
            "light" -> ControlKind.LIGHT
            "switch" -> ControlKind.SWITCH
            else -> ControlKind.OTHER
        }
        val actions = (o["actions"] as? JsonArray).orEmpty()
            .mapNotNull { ControlAction.fromWire(it.jsonPrimitive.content) }
            .toSet()
        val power = when (text("state")) {
            "on" -> PowerState.ON
            "off" -> PowerState.OFF
            else -> null
        }
        val id = text("id")
        require(id.isNotEmpty()) { "entry without id" }
        return ControlEntry(
            id = id,
            kind = kind,
            title = text("title").ifEmpty { id },
            room = text("room"),
            icon = text("icon"),
            actions = actions,
            power = power,
            available = o["available"]?.jsonPrimitive?.booleanOrNull ?: false,
        )
    }
}

/**
 * OkHttp implementation. Same safety rules as [ConnectSchedulerClient]: no
 * redirects (the bearer token must never follow a Location), token only in the
 * Authorization header, system trust store.
 */
class HttpControlCatalogClient(
    private val settings: ConnectionSettingsStore,
    private val baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) : ControlCatalogClient {
    private val json = Json { ignoreUnknownKeys = true }

    override suspend fun fetch(): Outcome<ControlCatalog> = withContext(ioDispatcher) {
        val config = settings.config.value
        val endpoint = config.controlEndpoint
            ?: return@withContext Outcome.Failure(RemoteFailure.Validation(Field.CONTROL_ENDPOINT))
        val token = config.bearerToken?.trim()
        if (!ConnectionConfig.isTokenValid(token)) {
            return@withContext Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }
        val url = "${endpoint.baseUrl()}/v1/catalog".toHttpUrlOrNull()
            ?: return@withContext Outcome.Failure(RemoteFailure.Validation(Field.CONTROL_ENDPOINT))

        val connectMillis = config.connectTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        val readMillis = config.readTimeoutMillis
            .coerceIn(ConnectionConfig.MIN_TIMEOUT_MILLIS, ConnectionConfig.MAX_TIMEOUT_MILLIS)
        val client = baseClient.newBuilder()
            .connectTimeout(connectMillis, TimeUnit.MILLISECONDS)
            .readTimeout(readMillis, TimeUnit.MILLISECONDS)
            .callTimeout(connectMillis + readMillis, TimeUnit.MILLISECONDS)
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .build()

        val request = Request.Builder()
            .url(url)
            .get()
            .header("Accept", "application/json")
            .header("Authorization", "Bearer $token")
            .build()

        val response = try {
            client.newCall(request).await()
        } catch (e: IOException) {
            return@withContext Outcome.Failure(e.toRemoteFailure())
        }
        response.use { result ->
            val code = result.code
            if (code in 300..399) {
                return@use Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP $code redirect refused"))
            }
            val text = try {
                readBoundedBody(result)
            } catch (_: ResponseTooLargeException) {
                return@use Outcome.Failure(RemoteFailure.ProtocolFailure("response exceeds 8 MiB limit"))
            } catch (e: IOException) {
                return@use Outcome.Failure(e.toRemoteFailure())
            }
            when {
                code == 401 || code == 403 -> Outcome.Failure(RemoteFailure.Unauthorized)
                code !in 200..299 -> Outcome.Failure(RemoteFailure.Server("http_$code", "server returned HTTP $code"))
                else -> try {
                    Outcome.Success(ControlJson.parseCatalog(json.parseToJsonElement(text)))
                } catch (e: Exception) {
                    Outcome.Failure(
                        RemoteFailure.ProtocolFailure(sanitizeMessage(e.message) ?: "unexpected response shape"),
                    )
                }
            }
        }
    }
}
