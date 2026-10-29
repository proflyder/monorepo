package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.Interceptor
import okhttp3.OkHttpClient

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

    /** Payload of a light adjustment: only the fields that are set are sent. */
    fun adjustPayload(entryId: String, adjust: LightAdjust): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", ControlAction.SET.wire)
            adjust.brightnessPct?.let { put("brightness", it) }
            adjust.kelvin?.let { put("kelvin", it) }
            adjust.hue?.let { put("hue", it) }
            adjust.saturation?.let { put("saturation", it) }
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
        val light = (o["light"] as? JsonObject)?.let { l ->
            fun num(name: String) = (l[name] as? JsonPrimitive)?.intOrNull ?: 0
            fun flag(name: String) = (l[name] as? JsonPrimitive)?.booleanOrNull ?: false
            LightControls(
                brightness = flag("brightness"),
                colorTemp = flag("colorTemp"),
                color = flag("color"),
                minKelvin = num("minKelvin"),
                maxKelvin = num("maxKelvin"),
                brightnessPct = num("brightnessPct"),
                kelvin = num("kelvin"),
                hue = num("hue"),
                saturation = num("saturation"),
            )
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
            light = light,
        )
    }
}

/**
 * Retrofit implementation ([ControlApi]). Same safety rules as
 * [ConnectSchedulerClient]: no redirects (the bearer token must never follow a
 * Location), token only in the Authorization header, system trust store.
 */
class HttpControlCatalogClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    /** Extra application interceptors (logging, inspection); they run after the connection interceptor. */
    interceptors: List<Interceptor> = emptyList(),
) : ControlCatalogClient {
    private val api: ControlApi = HomeRetrofit.create(
        ControlApi::class.java,
        settings,
        baseClient,
        Target.CONTROL,
        interceptors,
    )

    override suspend fun fetch(): Outcome<ControlCatalog> = withContext(ioDispatcher) {
        val response = try {
            api.catalog()
        } catch (e: IOException) {
            return@withContext Outcome.Failure(e.toRemoteFailure())
        } catch (e: SerializationException) {
            return@withContext Outcome.Failure(RemoteFailure.ProtocolFailure("invalid JSON response"))
        }
        val code = response.code()
        response.errorBody()?.close()
        when {
            code in 300..399 -> Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP $code redirect refused"))
            code == 401 || code == 403 -> Outcome.Failure(RemoteFailure.Unauthorized)
            !response.isSuccessful -> Outcome.Failure(RemoteFailure.Server("http_$code", "server returned HTTP $code"))
            else -> {
                val element = response.body()
                    ?: return@withContext Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
                try {
                    Outcome.Success(ControlJson.parseCatalog(element))
                } catch (e: Exception) {
                    Outcome.Failure(RemoteFailure.ProtocolFailure(sanitizeMessage(e.message) ?: "unexpected response shape"))
                }
            }
        }
    }
}
