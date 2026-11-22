package dev.proflyder.home.core.model

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonBuilder

val LocalJson: Json = Json { tolerant() }

private fun JsonBuilder.tolerant() {
    ignoreUnknownKeys = true
    coerceInputValues = true
    explicitNulls = false
    encodeDefaults = true
}
