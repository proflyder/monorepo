package dev.proflyder.home.core.model

import kotlinx.serialization.json.Json

val LocalJson: Json = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
}
