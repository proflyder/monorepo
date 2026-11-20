package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.VisibleWhen
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.Interceptor
import okhttp3.OkHttpClient

interface ControlCatalogClient {
    suspend fun fetch(): Outcome<ControlCatalog>
}

object ControlJson {
    const val QUEUE = "control"
    const val TASK_TYPE = "home.control"

    fun commandPayload(entryId: String, action: ControlAction): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", action.wire)
        }.toString()

    fun speakPayload(entryId: String, text: String): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", ControlAction.SPEAK.wire)
            put("text", text)
        }.toString()

    fun adjustPayload(entryId: String, adjust: LightAdjust): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", ControlAction.SET.wire)
            adjust.brightnessPct?.let { put("brightness", it) }
            adjust.kelvin?.let { put("kelvin", it) }
            adjust.hue?.let { put("hue", it) }
            adjust.saturation?.let { put("saturation", it) }
        }.toString()

    fun volumePayload(entryId: String, level: Int): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", ControlAction.SET.wire)
            put("level", level)
        }.toString()

    fun mutePayload(entryId: String, muted: Boolean): String =
        buildJsonObject {
            put("entry", entryId)
            put("action", if (muted) ControlAction.MUTE.wire else ControlAction.UNMUTE.wire)
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
        volume = volume?.toDomain(),
        deviceId = deviceIdSnake.ifEmpty { deviceId },
        device = device,
        role = if (role == "setting") ControlRole.SETTING else ControlRole.PRIMARY,
        visibleWhen = visibleWhen?.let { VisibleWhen(it.entry, it.state) },
    )
}

internal fun VolumeControlsDto.toDomain(): VolumeControls = VolumeControls(
    level = level,
    muted = muted,
)

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
    "volume" -> ControlKind.VOLUME
    else -> ControlKind.OTHER
}

private fun String?.toPowerState(): PowerState? = when (this) {
    "on" -> PowerState.ON
    "off" -> PowerState.OFF
    else -> null
}

class HttpControlCatalogClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
    networkWaiter: NetworkWaiter = NetworkWaiter.Online,
) : ControlCatalogClient {
    private val api: ControlApi = HomeRetrofit.create(
        ControlApi::class.java,
        settings,
        baseClient,
        Target.CONTROL,
        interceptors,
        networkWaiter,
    )

    override suspend fun fetch(): Outcome<ControlCatalog> =
        withContext(ioDispatcher) { api.catalog().mapResult { it.toDomain() } }
}
