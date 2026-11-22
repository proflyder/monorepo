package dev.proflyder.home.feature.scenarios.data

import dev.proflyder.home.core.model.LightParams
import dev.proflyder.home.core.network.WireJson
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

internal object CatalogStepParams {
    const val EMPTY = "{}"

    fun speak(text: String): String = WireJson.encodeToString(SpeakParams.serializer(), SpeakParams(text))

    fun speakText(raw: String): String = decode(raw, SpeakParams.serializer())?.text.orEmpty()

    fun light(params: LightParams): String = WireJson.encodeToString(LightParamsWire.serializer(), params.toWire())

    fun lightParams(raw: String): LightParams = decode(raw, LightParamsWire.serializer())?.toModel() ?: LightParams()

    private fun <T> decode(raw: String, serializer: kotlinx.serialization.KSerializer<T>): T? =
        runCatching { WireJson.decodeFromString(serializer, raw.ifBlank { EMPTY }) }.getOrNull()

    @Serializable
    private data class SpeakParams(val text: String = "")

    @Serializable
    private data class LightParamsWire(
        val brightness: Int? = null,
        val kelvin: Int? = null,
        val effect: String? = null,
        @SerialName("transition") val transitionSeconds: Double? = null,
    )

    private fun LightParams.toWire() = LightParamsWire(brightnessPct, kelvin, effect, transitionSeconds)

    private fun LightParamsWire.toModel() = LightParams(
        brightnessPct = brightness,
        kelvin = kelvin,
        effect = effect?.takeIf { it.isNotBlank() },
        transitionSeconds = transitionSeconds,
    )
}
