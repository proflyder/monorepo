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
import dev.proflyder.home.core.operation.suspendRunCatching
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSource

interface IntercomClient {
    suspend fun registerEndpoint(displayName: String, capabilities: List<String>): Outcome<IntercomEndpoint>
    suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>>
    suspend fun startCall(calleeEndpointId: String, idempotencyKey: String): Outcome<IntercomCall>
    suspend fun answer(callId: String): Outcome<IntercomCall>
    suspend fun hangup(callId: String, reason: IntercomEndReason = IntercomEndReason.UNSPECIFIED): Outcome<IntercomCall>
    suspend fun sendSignal(callId: String, signal: IntercomOutgoingSignal): Outcome<Boolean>
    suspend fun getIceConfig(): Outcome<List<IntercomIceServer>>
    suspend fun getStatus(): Outcome<IntercomStatus>

    fun watch(): Flow<IntercomWatchEvent>
}

class ConnectIntercomClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
    networkWaiter: NetworkWaiter = NetworkWaiter.Online,
) : IntercomClient {

    private val api: IntercomApi = HomeRetrofit.create(
        IntercomApi::class.java,
        settings,
        baseClient,
        Target.INTERCOM,
        interceptors,
        networkWaiter,
    )

    private val streamClient: OkHttpClient = baseClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(false)
        .addInterceptor(ConnectionInterceptor(settings, Target.INTERCOM))
        .build()

    override suspend fun registerEndpoint(
        displayName: String,
        capabilities: List<String>,
    ): Outcome<IntercomEndpoint> {
        val request = IntercomJson.registerEndpointRequest(displayName, capabilities)
        return io { api.registerEndpoint(request).mapResult { it.endpoint.toDomain() } }
    }

    override suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>> = io {
        api.listEndpoints(EmptyBody).mapResult { it.endpoints.map(EndpointDto::toDomain) }
    }

    override suspend fun startCall(
        calleeEndpointId: String,
        idempotencyKey: String,
    ): Outcome<IntercomCall> {
        val request = IntercomJson.startCallRequest(calleeEndpointId, idempotencyKey)
        return io { api.startCall(request).mapResult { it.call.toDomain() } }
    }

    override suspend fun answer(callId: String): Outcome<IntercomCall> = io {
        api.answer(CallRequestDto(callId)).mapResult { it.call.toDomain() }
    }

    override suspend fun hangup(
        callId: String,
        reason: IntercomEndReason,
    ): Outcome<IntercomCall> {
        val request = IntercomJson.hangupRequest(callId, reason)
        return io { api.hangup(request).mapResult { it.call.toDomain() } }
    }

    override suspend fun sendSignal(
        callId: String,
        signal: IntercomOutgoingSignal,
    ): Outcome<Boolean> {
        val request = IntercomJson.sendSignalRequest(callId, signal)
        return io { api.sendSignal(request).mapResult { it.delivered } }
    }

    override suspend fun getIceConfig(): Outcome<List<IntercomIceServer>> = io {
        api.getIceConfig(EmptyBody).mapResult { response ->
            response.iceServers.map(IceServerDto::toDomain)
        }
    }

    override suspend fun getStatus(): Outcome<IntercomStatus> = io {
        api.getStatus(EmptyBody).mapResult { it.toDomain() }
    }

    override fun watch(): Flow<IntercomWatchEvent> = channelFlow {
        var backoffMillis = INITIAL_BACKOFF_MILLIS
        while (currentCoroutineContext().isActive) {
            suspendRunCatching {
                withContext(ioDispatcher) {
                    streamWatch { event -> send(event) }
                }
            }.onSuccess {
                backoffMillis = INITIAL_BACKOFF_MILLIS
            }.onFailure { failure ->
                when (failure) {
                    is CancellationException -> throw failure
                    is IOException, is SerializationException -> Unit
                    else -> throw failure
                }
            }
            delay(backoffMillis)
            backoffMillis = (backoffMillis * 2).coerceAtMost(MAX_BACKOFF_MILLIS)
        }
    }

    private suspend fun streamWatch(onEvent: suspend (IntercomWatchEvent) -> Unit) {
        val body = ConnectFrames.encode(WireJson.encodeToString(EmptyBody.serializer(), EmptyBody))
        val request = Request.Builder()
            .url("$PLACEHOLDER_BASE_URL$SERVICE_PATH/Watch")
            .header(STREAM_REQUEST_HEADER, "1")
            .header("Accept", CONNECT_STREAM_MEDIA_TYPE)
            .post(body.toRequestBody(CONNECT_STREAM_MEDIA_TYPE.toMediaType()))
            .build()

        val call = streamClient.newCall(request)
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
        val source = response.body.source()
        while (true) {
            val frame = ConnectFrames.read(source) ?: return
            if (frame.endStream) return
            val event = WireJson.decodeFromString(WatchEventDto.serializer(), frame.payload).toDomain()
            if (event != null) onEvent(event)
        }
    }

    private suspend fun <T> io(block: () -> Outcome<T>): Outcome<T> = withContext(ioDispatcher) { block() }

    private companion object {
        const val PLACEHOLDER_BASE_URL = "http://localhost/"
        const val SERVICE_PATH = "home.v1.IntercomService"
        const val CONNECT_STREAM_MEDIA_TYPE = "application/connect+json"
        const val INITIAL_BACKOFF_MILLIS = 1_000L
        const val MAX_BACKOFF_MILLIS = 30_000L
    }
}

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
