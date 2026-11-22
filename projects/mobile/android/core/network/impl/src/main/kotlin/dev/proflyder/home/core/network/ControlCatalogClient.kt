package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.VisibleWhen
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.Outcome
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import kotlinx.serialization.json.JsonElement

internal fun parseControlCatalog(element: JsonElement): ControlCatalog =
    NetworkJson.decodeFromJsonElement(ControlCatalogDto.serializer(), element).toDomain()

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

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpControlCatalogClient(private val http: HomeHttp) : ControlCatalogClient {
    private val api: ControlApi by lazy { http.rest(ControlApi::class, Target.Control) }

    override suspend fun fetch(): Outcome<ControlCatalog> =
        api.catalog().mapResult { it.toDomain() }
}
