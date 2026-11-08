package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull

/**
 * Endpoint and timeout state supplied by Firebase Remote Config.
 *
 * The bearer token is deliberately absent: it is a device-local secret and is
 * never read from or written to Remote Config.
 */
sealed interface RemoteConnectionState {
    /** Firebase is not configured in this build (no google-services resources). */
    data object Unavailable : RemoteConnectionState

    /** A fetch is in flight and no usable value has been loaded yet. */
    data object Loading : RemoteConnectionState

    /** The Remote Config parameter is absent or empty. */
    data object Missing : RemoteConnectionState

    /**
     * A Remote Config value is present but cannot be used. [field] identifies a
     * bad URL/timeout; it is null for malformed JSON where no field can be
     * determined.
     */
    data class Invalid(val field: Field? = null, val detail: String = "") : RemoteConnectionState

    /**
     * The last fetch failed. [lastKnown] is the most recent successfully
     * activated configuration, if any, and remains usable for requests.
     */
    data class FetchFailed(val lastKnown: Loaded? = null) : RemoteConnectionState

    data class Loaded(
        val endpoint: ServerEndpoint,
        val controlEndpoint: ServerEndpoint?,
        val intercomEndpoint: ServerEndpoint?,
        val connectTimeoutMillis: Long,
        val readTimeoutMillis: Long,
    ) : RemoteConnectionState
}

/** Remote Config parameter carrying the JSON connection document. */
const val REMOTE_CONNECTION_KEY = "home_connection"

private val remoteJson = Json { ignoreUnknownKeys = true }

/**
 * Parses the single `home_connection` Remote Config string.
 *
 * Schema:
 * ```json
 * {
 *   "schedulerBaseUrl": "https://alarms.example.test",
 *   "controlBaseUrl": "https://control.example.test",
 *   "intercomBaseUrl": "https://intercom.example.test",
 *   "connectTimeoutMs": 5000,
 *   "readTimeoutMs": 20000
 * }
 * ```
 *
 * An empty string means "not configured". Unknown keys are ignored. Any
 * malformed value is reported as [RemoteConnectionState.Invalid] and never
 * throws.
 */
fun parseRemoteConnection(raw: String): RemoteConnectionState {
    if (raw.isBlank()) return RemoteConnectionState.Missing

    val document = runCatching {
        remoteJson.parseToJsonElement(raw).jsonObject
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

/** A required string field; null when absent, JSON null, or not a primitive. */
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

/**
 * Provider of endpoint/timeout configuration. Implementations own the transport
 * (Firebase in production, a fake in tests); consumers only see states.
 */
interface RemoteConnectionSource {
    val state: StateFlow<RemoteConnectionState>

    /** Fetches and activates the latest values; errors are reflected in [state]. */
    suspend fun refresh()
}
