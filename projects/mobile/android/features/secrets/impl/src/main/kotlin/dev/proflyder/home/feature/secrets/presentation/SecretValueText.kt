package dev.proflyder.home.feature.secrets.presentation

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

internal object SecretValueText {
    private val compact = Json
    private val pretty = Json { prettyPrint = true }
    private val whitespace = Regex("\\s+")

    fun preview(raw: String): String = structured(raw)?.let { compact.encodeToString(JsonElement.serializer(), it) }
        ?: raw.trim().replace(whitespace, " ")

    fun full(raw: String): String? = structured(raw)?.let { pretty.encodeToString(JsonElement.serializer(), it) }
        ?: raw.takeIf { it.trim().contains('\n') }

    private fun structured(raw: String): JsonElement? = runCatching { Json.parseToJsonElement(raw) }
        .getOrNull()
        ?.takeIf { it is JsonObject || it is JsonArray }
}
