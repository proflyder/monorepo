package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.LightAdjust
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal object ControlTaskPayloads {
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
}
