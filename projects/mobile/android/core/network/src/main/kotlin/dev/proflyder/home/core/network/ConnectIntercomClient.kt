package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.IntercomCall
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomStatus
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.operation.suspendRunCatching
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSource

/** Transport-level contract for the intercom signaling service. */
interface IntercomClient {
    suspend fun registerEndpoint(displayName: String, capabilities: List<String>): Outcome<IntercomEndpoint>
    suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>>
    suspend fun startCall(calleeEndpointId: String, idempotencyKey: String): Outcome<IntercomCall>
    suspend fun answer(callId: String): Outcome<IntercomCall>
    suspend fun hangup(callId: String, reason: IntercomEndReason = IntercomEndReason.UNSPECIFIED): Outcome<IntercomCall>
    suspend fun sendSignal(callId: String, signal: IntercomOutgoingSignal): Outcome<Boolean>
    suspend fun getIceConfig(): Outcome<List<IntercomIceServer>>
    suspend fun getStatus(): Outcome<IntercomStatus>

    /**
     * Opens the long-lived server-streaming `Watch` RPC and reconnects with a
     * bounded backoff when it drops. The flow never completes on its own; the
     * caller cancels it. The first connection marks this endpoint online.
     */
    fun watch(): Flow<IntercomWatchEvent>
}

/**
 * Connect JSON client for home.v1.IntercomService.
 *
 * Unary RPCs use `application/json`; `Watch` is server streaming and therefore
 * uses the Connect streaming envelope framing over HTTP/1.1
 * (`application/connect+json`): a 1-byte flags word, a 4-byte big-endian
 * message length, then the JSON message. This avoids HTTP/2 and WebSockets.
 *
 * Security properties match the scheduler client: redirects are refused, the
 * token is only in the Authorization header, and the system TLS trust store is
 * used.
 */
class ConnectIntercomClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
) : IntercomClient {

    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = false
    }

    private val api: IntercomApi = HomeRetrofit.create(
        IntercomApi::class.java,
        settings,
        baseClient,
        Target.INTERCOM,
        interceptors,
    )

    private val streamClient: OkHttpClient = baseClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(false)
        .addInterceptor(ConnectionInterceptor(settings, Target.INTERCOM))
        .apply { interceptors.forEach { addInterceptor(it) } }
        .build()

    override suspend fun registerEndpoint(
        displayName: String,
        capabilities: List<String>,
    ): Outcome<IntercomEndpoint> = call(
        "RegisterEndpoint",
        IntercomJson.registerEndpointRequest(displayName, capabilities),
    ) { element ->
        IntercomJson.parseEndpoint(element.jsonObject.getValue("endpoint"))
    }

    override suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>> =
        call("ListEndpoints", buildJsonObject { }) { element ->
            val array = element.jsonObject["endpoints"] as? JsonArray ?: return@call emptyList()
            array.map(IntercomJson::parseEndpoint)
        }

    override suspend fun startCall(
        calleeEndpointId: String,
        idempotencyKey: String,
    ): Outcome<IntercomCall> = call(
        "StartCall",
        IntercomJson.startCallRequest(calleeEndpointId, idempotencyKey),
    ) { element ->
        IntercomJson.parseCall(element.jsonObject.getValue("call"))
    }

    override suspend fun answer(callId: String): Outcome<IntercomCall> =
        call("Answer", buildJsonObject { put("callId", callId) }) { element ->
            IntercomJson.parseCall(element.jsonObject.getValue("call"))
        }

    override suspend fun hangup(
        callId: String,
        reason: IntercomEndReason,
    ): Outcome<IntercomCall> = call(
        "Hangup",
        IntercomJson.hangupRequest(callId, reason),
    ) { element ->
        IntercomJson.parseCall(element.jsonObject.getValue("call"))
    }

    override suspend fun sendSignal(
        callId: String,
        signal: IntercomOutgoingSignal,
    ): Outcome<Boolean> = call(
        "SendSignal",
        IntercomJson.sendSignalRequest(callId, signal),
    ) { element ->
        (element.jsonObject["delivered"]?.toString() == "true")
    }

    override suspend fun getIceConfig(): Outcome<List<IntercomIceServer>> =
        call("GetIceConfig", buildJsonObject { }) { element -> IntercomJson.parseIceServers(element) }

    override suspend fun getStatus(): Outcome<IntercomStatus> =
        call("GetStatus", buildJsonObject { }) { element -> IntercomJson.parseStatus(element) }

    override fun watch(): Flow<IntercomWatchEvent> = channelFlow {
        var backoffMillis = INITIAL_BACKOFF_MILLIS
        while (currentCoroutineContext().isActive) {
            suspendRunCatching {
                withContext(ioDispatcher) {
                    streamWatch { event -> send(event) }
                }
            }.onSuccess {
                // A clean end is unusual for a permanent Watch; reconnect too.
                backoffMillis = INITIAL_BACKOFF_MILLIS
            }.onFailure { failure ->
                when (failure) {
                    is CancellationException -> throw failure
                    // Offline/TLS/stream reset, or a malformed frame: drop this
                    // stream and reconnect with backoff.
                    is IOException, is SerializationException -> Unit
                    else -> throw failure
                }
            }
            delay(backoffMillis)
            backoffMillis = (backoffMillis * 2).coerceAtMost(MAX_BACKOFF_MILLIS)
        }
    }

    private suspend fun streamWatch(onEvent: suspend (IntercomWatchEvent) -> Unit) {
        val body = ConnectFrames.encode(json.encodeToString(JsonObject.serializer(), buildJsonObject { }))
        val request = Request.Builder()
            .url("$PLACEHOLDER_BASE_URL$SERVICE_PATH/Watch")
            .header(STREAM_REQUEST_HEADER, "1")
            .header("Accept", CONNECT_STREAM_MEDIA_TYPE)
            .post(body.toRequestBody(CONNECT_STREAM_MEDIA_TYPE.toMediaType()))
            .build()

        val call = streamClient.newCall(request)
        // A blocking stream read does not react to coroutine cancellation by
        // itself; cancel the OkHttp call so the Watch socket closes promptly.
        val cancellation = currentCoroutineContext()[Job]?.invokeOnCompletion { call.cancel() }
        runCatching { readStream(call, onEvent) }
            .also { cancellation?.dispose() }
            .getOrThrow()
    }

    private suspend fun readStream(
        call: okhttp3.Call,
        onEvent: suspend (IntercomWatchEvent) -> Unit,
    ) {
        call.execute().use { response -> readResponse(response, onEvent) }
    }

    private suspend fun readResponse(
        response: okhttp3.Response,
        onEvent: suspend (IntercomWatchEvent) -> Unit,
    ) {
        if (response.code in 300..399) {
            throw IOException("HTTP ${response.code} redirect refused")
        }
        if (!response.isSuccessful) {
            throw IOException("HTTP ${response.code}")
        }
        val source = response.body?.source() ?: throw IOException("empty stream")
        while (true) {
            val frame = ConnectFrames.read(source) ?: return
            if (frame.endStream) return
            val element = json.parseToJsonElement(frame.payload)
            val event = IntercomJson.parseWatchEvent(element)
            if (event != null) onEvent(event)
        }
    }

    private suspend fun <T> call(
        method: String,
        body: JsonObject,
        decode: (JsonElement) -> T,
    ): Outcome<T> = withContext(ioDispatcher) {
        val response = runCatching { api.call(method, body) }.getOrElse { failure ->
            when (failure) {
                is IOException, is SerializationException ->
                    return@withContext Outcome.Failure(failure.toCallFailure())
                else -> throw failure
            }
        }

        val code = response.code()
        when {
            code in 300..399 -> Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP $code redirect refused"))
            code == 401 || code == 403 -> Outcome.Failure(RemoteFailure.Unauthorized)
            !response.isSuccessful -> {
                val text = runCatching {
                    response.errorBody()?.use { it.readBounded() }.orEmpty()
                }.getOrElse { failure ->
                    if (failure is IOException) {
                        return@withContext Outcome.Failure(failure.toRemoteFailure())
                    }
                    throw failure
                }
                Outcome.Failure(parseConnectError(code, text))
            }
            else -> {
                val element = response.body()
                    ?: return@withContext Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
                runCatching { decode(element) }.fold(
                    onSuccess = { Outcome.Success(it) },
                    onFailure = { failure ->
                        if (failure !is Exception) throw failure
                        Outcome.Failure(
                            RemoteFailure.ProtocolFailure(
                                sanitizeMessage(failure.message) ?: "unexpected response shape",
                            ),
                        )
                    },
                )
            }
        }
    }

    private fun parseConnectError(httpCode: Int, text: String): RemoteFailure {
        val parsed = runCatching {
            if (text.isBlank()) null else json.parseToJsonElement(text).jsonObject
        }.getOrNull()
        val code = sanitizeMessage((parsed?.get("code") as? kotlinx.serialization.json.JsonPrimitive)?.content)
            .orEmpty()
        val message = sanitizeMessage((parsed?.get("message") as? kotlinx.serialization.json.JsonPrimitive)?.content)
            .orEmpty()
        return when (code) {
            "unauthenticated", "permission_denied" -> RemoteFailure.Unauthorized
            "deadline_exceeded", "canceled" -> RemoteFailure.Timeout
            "unavailable" -> RemoteFailure.Offline(message.ifBlank { "server unavailable" })
            else -> RemoteFailure.Server(
                code = code.ifBlank { "http_$httpCode" },
                message = message.ifBlank { "server returned HTTP $httpCode" },
            )
        }
    }

    private companion object {
        const val PLACEHOLDER_BASE_URL = "http://localhost/"
        const val SERVICE_PATH = "home.v1.IntercomService"
        const val CONNECT_STREAM_MEDIA_TYPE = "application/connect+json"
        const val INITIAL_BACKOFF_MILLIS = 1_000L
        const val MAX_BACKOFF_MILLIS = 30_000L
    }
}

/** Connect streaming framing: 1 flag byte + 4-byte big-endian length. */
object ConnectFrames {
    private const val FLAG_END_STREAM = 0x01
    private const val MAX_FRAME_BYTES = 8 * 1024 * 1024

    data class Frame(val endStream: Boolean, val payload: String)

    fun encode(payload: String): ByteArray {
        val bytes = payload.toByteArray(Charsets.UTF_8)
        val out = ByteArray(5 + bytes.size)
        out[0] = 0
        out[1] = (bytes.size ushr 24).toByte()
        out[2] = (bytes.size ushr 16).toByte()
        out[3] = (bytes.size ushr 8).toByte()
        out[4] = bytes.size.toByte()
        bytes.copyInto(out, 5)
        return out
    }

    /** Returns null on a clean end of input. */
    fun read(source: BufferedSource): Frame? {
        if (source.exhausted()) return null
        val header = source.readByteArray(5L)
        val flags = header[0].toInt() and 0xFF
        val length = ((header[1].toInt() and 0xFF) shl 24) or
            ((header[2].toInt() and 0xFF) shl 16) or
            ((header[3].toInt() and 0xFF) shl 8) or
            (header[4].toInt() and 0xFF)
        if (length < 0 || length > MAX_FRAME_BYTES) throw IOException("invalid Connect frame length")
        if (length == 0) return Frame(flags and FLAG_END_STREAM != 0, "")
        val payload = source.readByteArray(length.toLong()).toString(Charsets.UTF_8)
        return Frame(flags and FLAG_END_STREAM != 0, payload)
    }
}
