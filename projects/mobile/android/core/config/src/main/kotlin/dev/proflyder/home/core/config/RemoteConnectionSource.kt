package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.decodeFromJsonElement

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

@Serializable
internal data class RemoteConnectionDocument(
    @SerialName(SCHEDULER_KEY) val scheduler: String? = null,
    @SerialName(CONTROL_KEY) val control: String? = null,
    @SerialName(INTERCOM_KEY) val intercom: String? = null,
    @SerialName(CONNECT_TIMEOUT_KEY) val connectTimeout: Long? = null,
    @SerialName(READ_TIMEOUT_KEY) val readTimeout: Long? = null,
)

private class InvalidDocument(val state: RemoteConnectionState.Invalid) : Exception()

private const val MALFORMED_DETAIL = "Malformed JSON"
private const val SCHEDULER_KEY = "schedulerBaseUrl"
private const val CONTROL_KEY = "controlBaseUrl"
private const val INTERCOM_KEY = "intercomBaseUrl"
private const val CONNECT_TIMEOUT_KEY = "connectTimeoutMs"
private const val READ_TIMEOUT_KEY = "readTimeoutMs"

private val DocumentJson = Json { ignoreUnknownKeys = true }

private val DOCUMENT_FIELDS = listOf(
    DocumentField(SCHEDULER_KEY, Field.ENDPOINT, String.serializer().nullable),
    DocumentField(CONTROL_KEY, Field.CONTROL_ENDPOINT, String.serializer().nullable),
    DocumentField(INTERCOM_KEY, Field.INTERCOM_ENDPOINT, String.serializer().nullable),
    DocumentField(CONNECT_TIMEOUT_KEY, Field.TIMEOUT, Long.serializer().nullable),
    DocumentField(READ_TIMEOUT_KEY, Field.TIMEOUT, Long.serializer().nullable),
)

fun parseRemoteConnection(raw: String): RemoteConnectionState {
    if (raw.isBlank()) return RemoteConnectionState.Missing
    val tree = runCatching { DocumentJson.parseToJsonElement(raw) }
        .getOrElse { return RemoteConnectionState.Invalid(detail = MALFORMED_DETAIL) }
    return runCatching { DocumentJson.decodeFromJsonElement<RemoteConnectionDocument>(tree).toState() }
        .getOrElse { invalidState(it, tree) }
}

private fun invalidState(failure: Throwable, tree: JsonElement): RemoteConnectionState.Invalid = when (failure) {
    is InvalidDocument -> failure.state
    is SerializationException -> RemoteConnectionState.Invalid(mistypedField(tree), failure.message.orEmpty())
    else -> throw failure
}

private fun mistypedField(tree: JsonElement): Field? {
    val document = tree as? JsonObject ?: return null
    return DOCUMENT_FIELDS.firstOrNull { !it.accepts(document[it.key]) }?.field
}

private class DocumentField(val key: String, val field: Field, val serializer: KSerializer<*>) {
    fun accepts(element: JsonElement?): Boolean =
        element == null || runCatching { DocumentJson.decodeFromJsonElement(serializer, element) }.isSuccess
}

private fun RemoteConnectionDocument.toState(): RemoteConnectionState.Loaded {
    val timeouts = ConnectionConfig.MIN_TIMEOUT_MILLIS..ConnectionConfig.MAX_TIMEOUT_MILLIS
    val connect = connectTimeout ?: ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS
    val read = readTimeout ?: ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS
    if (connect !in timeouts || read !in timeouts) invalid(Field.TIMEOUT)
    return RemoteConnectionState.Loaded(
        endpoint = parseOptionalEndpoint(scheduler, Field.ENDPOINT) ?: invalid(Field.ENDPOINT, "$SCHEDULER_KEY is required"),
        controlEndpoint = parseOptionalEndpoint(control, Field.CONTROL_ENDPOINT),
        intercomEndpoint = parseOptionalEndpoint(intercom, Field.INTERCOM_ENDPOINT),
        connectTimeoutMillis = connect,
        readTimeoutMillis = read,
    )
}

internal fun parseOptionalEndpoint(raw: String?, field: Field): ServerEndpoint? {
    if (raw.isNullOrBlank()) return null
    return when (val parsed = ServerEndpoint.parse(raw)) {
        is EndpointParseResult.Valid -> parsed.endpoint
        is EndpointParseResult.Invalid -> invalid(field)
    }
}

private fun invalid(field: Field, detail: String = ""): Nothing =
    throw InvalidDocument(RemoteConnectionState.Invalid(field, detail))

interface RemoteConnectionSource {
    val state: StateFlow<RemoteConnectionState>

    suspend fun refresh()
}
