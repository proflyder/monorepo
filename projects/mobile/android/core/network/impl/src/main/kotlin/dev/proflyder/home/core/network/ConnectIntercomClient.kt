package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.IntercomCall
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomStatus
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.runtime.Backoff
import dev.proflyder.home.core.runtime.reconnecting
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.SerializationException
import okio.BufferedSource

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectIntercomClient(private val http: HomeHttp) : IntercomClient {
    private val api: IntercomApi by lazy { http.rest(IntercomApi::class, Target.Intercom) }

    override suspend fun registerEndpoint(
        displayName: String,
        capabilities: List<String>,
    ): Outcome<IntercomEndpoint> = api.registerEndpoint(IntercomJson.registerEndpointRequest(displayName, capabilities))
        .mapResult { it.endpoint.toDomain() }

    override suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>> =
        api.listEndpoints(EmptyBody).mapResult { it.endpoints.map(EndpointDto::toDomain) }

    override suspend fun startCall(calleeEndpointId: String, idempotencyKey: String): Outcome<IntercomCall> =
        api.startCall(IntercomJson.startCallRequest(calleeEndpointId, idempotencyKey)).mapResult { it.call.toDomain() }

    override suspend fun answer(callId: String): Outcome<IntercomCall> =
        api.answer(CallRequestDto(callId)).mapResult { it.call.toDomain() }

    override suspend fun hangup(callId: String, reason: IntercomEndReason): Outcome<IntercomCall> =
        api.hangup(IntercomJson.hangupRequest(callId, reason)).mapResult { it.call.toDomain() }

    override suspend fun sendSignal(callId: String, signal: IntercomOutgoingSignal): Outcome<Boolean> =
        api.sendSignal(IntercomJson.sendSignalRequest(callId, signal)).mapResult { it.delivered }

    override suspend fun getIceConfig(): Outcome<List<IntercomIceServer>> =
        api.getIceConfig(EmptyBody).mapResult { response -> response.iceServers.map(IceServerDto::toDomain) }

    override suspend fun getStatus(): Outcome<IntercomStatus> =
        api.getStatus(EmptyBody).mapResult { it.toDomain() }

    override fun watch(): Flow<IntercomWatchEvent> =
        http.stream(Target.Intercom, watchRequest, ConnectWatchDecoder)
            .reconnecting(WATCH_BACKOFF) { it is IOException || it is SerializationException }

    private val watchRequest = StreamRequest(
        path = "home.v1.IntercomService/Watch",
        accept = CONNECT_STREAM_MEDIA_TYPE,
        body = ConnectFrames.encode(NetworkJson.encodeToString(EmptyBody.serializer(), EmptyBody)),
    )

    private companion object {
        const val CONNECT_STREAM_MEDIA_TYPE = "application/connect+json"
        val WATCH_BACKOFF = Backoff()
    }
}

internal object ConnectWatchDecoder : StreamDecoder<IntercomWatchEvent> {
    override suspend fun decode(source: BufferedSource, emit: suspend (IntercomWatchEvent) -> Unit) {
        generateSequence { ConnectFrames.read(source) }
            .takeWhile { !it.endStream }
            .mapNotNull { NetworkJson.decodeFromString(WatchEventDto.serializer(), it.payload).toDomain() }
            .forEach { emit(it) }
    }
}

internal object ConnectFrames {
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
