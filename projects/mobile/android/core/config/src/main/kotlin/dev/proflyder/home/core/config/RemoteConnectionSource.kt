package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import kotlinx.coroutines.flow.StateFlow
import dev.proflyder.home.core.model.LocalJson
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull

sealed interface RemoteConnectionState {
    data object Unavailable : RemoteConnectionState

    data object Loading : RemoteConnectionState

    data object Missing : RemoteConnectionState

    data class Invalid(val field: Field? = null, val detail: String = "") : RemoteConnectionState

    data class FetchFailed(val lastKnown: Loaded? = null) : RemoteConnectionState

    data class Loaded(
        val endpoint: ServerEndpoint,
        val controlEndpoint: ServerEndpoint?,
        val intercomEndpoint: ServerEndpoint?,
        val connectTimeoutMillis: Long,
        val readTimeoutMillis: Long,
    ) : RemoteConnectionState
}

const val REMOTE_CONNECTION_KEY = "home_connection"

fun parseRemoteConnection(raw: String): RemoteConnectionState {
    if (raw.isBlank()) return RemoteConnectionState.Missing

    val document = runCatching {
        LocalJson.parseToJsonElement(raw).jsonObject
    }.getOrElse {
        return RemoteConnectionState.Invalid(detail = "Malformed JSON")
    }

    val scheduler = document.stringValue("schedulerBaseUrl")
        ?: return RemoteConnectionState.Invalid(Field.ENDPOINT, "schedulerBaseUrl is required")
    if (scheduler.isBlank()) {
        return RemoteConnectionState.Invalid(Field.ENDPOINT, "schedulerBaseUrl is required")
    }

    val endpoint = when (val parsed = ServerEndpoint.parse(scheduler)) {
        is EndpointParseResult.Valid -> parsed.endpoint
        is EndpointParseResult.Invalid -> return RemoteConnectionState.Invalid(Field.ENDPOINT)
    }

    val controlElement = document["controlBaseUrl"]
    val control = when {
        controlElement == null || controlElement is JsonNull -> ""
        controlElement is JsonPrimitive -> controlElement.contentOrNull.orEmpty()
        else -> return RemoteConnectionState.Invalid(
            Field.CONTROL_ENDPOINT,
            "controlBaseUrl must be a string",
        )
    }
    val controlEndpoint = if (control.isBlank()) {
        null
    } else {
        when (val parsed = ServerEndpoint.parse(control)) {
            is EndpointParseResult.Valid -> parsed.endpoint
            is EndpointParseResult.Invalid -> return RemoteConnectionState.Invalid(Field.CONTROL_ENDPOINT)
        }
    }

    val intercomElement = document["intercomBaseUrl"]
    val intercom = when {
        intercomElement == null || intercomElement is JsonNull -> ""
        intercomElement is JsonPrimitive -> intercomElement.contentOrNull.orEmpty()
        else -> return RemoteConnectionState.Invalid(
            Field.INTERCOM_ENDPOINT,
            "intercomBaseUrl must be a string",
        )
    }
    val intercomEndpoint = if (intercom.isBlank()) {
        null
    } else {
        when (val parsed = ServerEndpoint.parse(intercom)) {
            is EndpointParseResult.Valid -> parsed.endpoint
            is EndpointParseResult.Invalid -> return RemoteConnectionState.Invalid(Field.INTERCOM_ENDPOINT)
        }
    }

    val connectTimeout = document.longValue(
        "connectTimeoutMs",
        ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS,
    ) ?: return RemoteConnectionState.Invalid(Field.TIMEOUT, "connectTimeoutMs must be a number")
    val readTimeout = document.longValue(
        "readTimeoutMs",
        ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS,
    ) ?: return RemoteConnectionState.Invalid(Field.TIMEOUT, "readTimeoutMs must be a number")

    val timeoutRange = ConnectionConfig.MIN_TIMEOUT_MILLIS..ConnectionConfig.MAX_TIMEOUT_MILLIS
    if (connectTimeout !in timeoutRange || readTimeout !in timeoutRange) {
        return RemoteConnectionState.Invalid(Field.TIMEOUT)
    }

    return RemoteConnectionState.Loaded(
        endpoint = endpoint,
        controlEndpoint = controlEndpoint,
        intercomEndpoint = intercomEndpoint,
        connectTimeoutMillis = connectTimeout,
        readTimeoutMillis = readTimeout,
    )
}

private fun JsonObject.stringValue(key: String): String? {
    val element = this[key] ?: return null
    if (element is JsonNull) return null
    val primitive = element as? JsonPrimitive ?: return null
    return primitive.contentOrNull
}

private fun JsonObject.longValue(key: String, default: Long): Long? {
    val element = this[key] ?: return default
    if (element is JsonNull) return default
    val primitive = element as? JsonPrimitive ?: return null
    return primitive.longOrNull
}

interface RemoteConnectionSource {
    val state: StateFlow<RemoteConnectionState>

    suspend fun refresh()
}
