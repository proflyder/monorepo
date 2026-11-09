package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
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

    /**
     * Payload of a speak action. The text is user content: it travels only in
     * the task payload and is never put in a URL, a log line or an error.
     */
    fun speakPayload(entryId: String, text: String): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", ControlAction.SPEAK.wire)
            put("text", text)
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

    fun parseCatalog(element: JsonElement): ControlCatalog =
        WireJson.decodeFromJsonElement(ControlCatalogDto.serializer(), element).toDomain()
}

internal fun ControlCatalogDto.toDomain(): ControlCatalog = ControlCatalog(
    entries = entries.map { it.toDomain() },
    statesOk = statesOk,
)

internal fun ControlEntryDto.toDomain(): ControlEntry {
    require(id.isNotEmpty()) { "entry without id" }
    return ControlEntry(
        id = id,
        kind = kind.toControlKind(),
        title = title.ifEmpty { id },
        titleKey = titleKey,
        titleArgs = titleArgs,
        detailKey = detailKey,
        room = room,
        icon = icon,
        actions = actions.mapNotNull(ControlAction::fromWire).toSet(),
        power = state.toPowerState(),
        available = available,
        light = light?.toDomain(),
        deviceId = deviceIdSnake.ifEmpty { deviceId },
        device = device,
        role = if (role == "setting") ControlRole.SETTING else ControlRole.PRIMARY,
    )
}

internal fun LightControlsDto.toDomain(): LightControls = LightControls(
    brightness = brightness,
    colorTemp = colorTemp,
    color = color,
    minKelvin = minKelvin,
    maxKelvin = maxKelvin,
    brightnessPct = brightnessPct,
    kelvin = kelvin,
    hue = hue,
    saturation = saturation,
    mode = mode,
    effects = effects,
)

private fun String.toControlKind(): ControlKind = when (this) {
    "scenario" -> ControlKind.SCENARIO
    "light" -> ControlKind.LIGHT
    "switch" -> ControlKind.SWITCH
    "button" -> ControlKind.BUTTON
    "profile" -> ControlKind.PROFILE
    "action" -> ControlKind.ACTION
    // An unknown kind stays OTHER, so an older app never drives a command it
    // does not understand.
    else -> ControlKind.OTHER
}

private fun String?.toPowerState(): PowerState? = when (this) {
    "on" -> PowerState.ON
    "off" -> PowerState.OFF
    else -> null
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

    override suspend fun fetch(): Outcome<ControlCatalog> =
        withContext(ioDispatcher) { api.catalog().mapResult { it.toDomain() } }
}
