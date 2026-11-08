package dev.proflyder.home.feature.call.media

import android.content.Context
import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.call.CallMedia
import dev.proflyder.home.feature.call.CallMediaEvent
import dev.proflyder.home.feature.call.CallMediaFactory
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import org.webrtc.AudioSource
import org.webrtc.AudioTrack
import org.webrtc.IceCandidate
import org.webrtc.audio.JavaAudioDeviceModule
import org.webrtc.MediaConstraints
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription

/**
 * Audio-only libwebrtc session for one call.
 *
 * Opus is negotiated by WebRTC's default audio codec set. Echo cancellation,
 * noise suppression and gain control are requested from
 * [JavaAudioDeviceModule]; hardware AEC/NS is used where the device reports
 * support and falls back to the software implementations otherwise.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class WebRtcCallMediaFactory(private val context: Context) : CallMediaFactory {

    override fun create(): CallMedia = WebRtcCallMedia(context.applicationContext)
}

private class WebRtcCallMedia(private val context: Context) : CallMedia {

    private val mutableEvents = MutableSharedFlow<CallMediaEvent>(extraBufferCapacity = 32)
    override val events: Flow<CallMediaEvent> = mutableEvents

    private val closed = AtomicBoolean(false)
    private var factory: PeerConnectionFactory? = null
    private var peerConnection: PeerConnection? = null
    private var audioSource: AudioSource? = null
    private var audioTrack: AudioTrack? = null

    override suspend fun prepare(iceServers: List<IntercomIceServer>): Outcome<Unit> =
        runCatching { prepareInternal(iceServers) }.getOrElse { failure ->
            close()
            Outcome.Failure(failure.toMediaFailure("prepare failed"))
        }

    private suspend fun prepareInternal(iceServers: List<IntercomIceServer>): Outcome<Unit> {
        val createdFactory = buildPeerConnectionFactory(context)
        factory = createdFactory

        val audioConstraints = MediaConstraints().apply {
            mandatory.add(MediaConstraints.KeyValuePair("googEchoCancellation", "true"))
            mandatory.add(MediaConstraints.KeyValuePair("googAutoGainControl", "true"))
            mandatory.add(MediaConstraints.KeyValuePair("googNoiseSuppression", "true"))
            mandatory.add(MediaConstraints.KeyValuePair("googHighpassFilter", "true"))
        }
        val source = createdFactory.createAudioSource(audioConstraints)
        audioSource = source
        val track = createdFactory.createAudioTrack(AUDIO_TRACK_ID, source)
        audioTrack = track
        track.setEnabled(true)

        val configuration = PeerConnection.RTCConfiguration(iceServers.map(::toIceServer)).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
        }
        val connection = createdFactory.createPeerConnection(configuration, Observer())
            ?: return Outcome.Failure(RemoteFailure.ProtocolFailure("webrtc: peer connection was not created"))
        peerConnection = connection
        connection.addTrack(track, listOf(AUDIO_STREAM_ID))
        return Outcome.Success(Unit)
    }

    override suspend fun createOffer(): Outcome<String> {
        val connection = peerConnection
            ?: return Outcome.Failure(RemoteFailure.ProtocolFailure("webrtc: not prepared"))
        return runCatching {
            val offer = createOffer(connection)
            setLocalDescription(connection, offer)
            Outcome.Success(offer.description)
        }.getOrElse { failure -> Outcome.Failure(failure.toMediaFailure("offer failed")) }
    }

    override suspend fun setRemoteAnswer(sdp: String): Outcome<Unit> {
        val connection = peerConnection
            ?: return Outcome.Failure(RemoteFailure.ProtocolFailure("webrtc: not prepared"))
        return runCatching {
            setRemoteDescription(connection, SessionDescription(SessionDescription.Type.ANSWER, sdp))
            Outcome.Success(Unit)
        }.getOrElse { failure -> Outcome.Failure(failure.toMediaFailure("answer failed")) }
    }

    override suspend fun addRemoteCandidate(candidate: IntercomIceCandidate): Outcome<Unit> {
        val connection = peerConnection
            ?: return Outcome.Failure(RemoteFailure.ProtocolFailure("webrtc: not prepared"))
        return runCatching {
            connection.addIceCandidate(
                IceCandidate(candidate.sdpMid, candidate.sdpMlineIndex, candidate.candidate),
            )
            Outcome.Success(Unit)
        }.getOrElse { failure -> Outcome.Failure(failure.toMediaFailure("candidate failed")) }
    }

    private fun Throwable.toMediaFailure(stage: String): RemoteFailure =
        RemoteFailure.ProtocolFailure("webrtc: ${message ?: stage}")

    override fun setMuted(muted: Boolean) {
        audioTrack?.setEnabled(!muted)
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        peerConnection?.close()
        peerConnection?.dispose()
        peerConnection = null
        audioTrack?.dispose()
        audioTrack = null
        audioSource?.dispose()
        audioSource = null
        factory?.dispose()
        factory = null
    }

    private inner class Observer : PeerConnection.Observer {
        override fun onIceCandidate(candidate: IceCandidate) {
            mutableEvents.tryEmit(
                CallMediaEvent.LocalCandidate(
                    IntercomIceCandidate(
                        candidate = candidate.sdp,
                        sdpMid = candidate.sdpMid ?: "",
                        sdpMlineIndex = candidate.sdpMLineIndex,
                    ),
                ),
            )
        }

        override fun onIceConnectionChange(newState: PeerConnection.IceConnectionState) {
            if (newState == PeerConnection.IceConnectionState.FAILED) {
                mutableEvents.tryEmit(CallMediaEvent.Failed)
            }
        }

        override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
        override fun onIceGatheringChange(newState: PeerConnection.IceGatheringState) = Unit
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
        override fun onSignalingChange(newState: PeerConnection.SignalingState) = Unit
        override fun onAddStream(stream: org.webrtc.MediaStream) = Unit
        override fun onRemoveStream(stream: org.webrtc.MediaStream) = Unit
        override fun onDataChannel(channel: org.webrtc.DataChannel) = Unit
        override fun onRenegotiationNeeded() = Unit
    }

    private fun toIceServer(server: IntercomIceServer): PeerConnection.IceServer {
        val url = server.urls.firstOrNull() ?: ""
        return PeerConnection.IceServer.builder(url)
            .setUsername(server.username)
            .setPassword(server.credential)
            .createIceServer()
    }

    private companion object {
        const val AUDIO_TRACK_ID = "home-call-audio"
        const val AUDIO_STREAM_ID = "home-call-stream"
    }
}

internal suspend fun createOffer(connection: PeerConnection): SessionDescription =
    suspendCancellableCoroutine { continuation ->
        connection.createOffer(
            object : SdpObserver {
                override fun onCreateSuccess(sdp: SessionDescription) {
                    if (continuation.isActive) continuation.resume(sdp)
                }

                override fun onCreateFailure(error: String?) {
                    if (continuation.isActive) {
                        continuation.resumeWithException(IllegalStateException(error ?: "createOffer failed"))
                    }
                }

                override fun onSetSuccess() = Unit
                override fun onSetFailure(error: String?) = Unit
            },
            MediaConstraints(),
        )
    }

internal suspend fun setLocalDescription(connection: PeerConnection, description: SessionDescription): Unit =
    suspendCancellableCoroutine { continuation ->
        connection.setLocalDescription(
            object : SdpObserver {
                override fun onSetSuccess() {
                    if (continuation.isActive) continuation.resume(Unit)
                }

                override fun onSetFailure(error: String?) {
                    if (continuation.isActive) {
                        continuation.resumeWithException(IllegalStateException(error ?: "setLocalDescription failed"))
                    }
                }

                override fun onCreateSuccess(description: SessionDescription?) = Unit
                override fun onCreateFailure(error: String?) = Unit
            },
            description,
        )
    }

internal suspend fun setRemoteDescription(connection: PeerConnection, description: SessionDescription): Unit =
    suspendCancellableCoroutine { continuation ->
        connection.setRemoteDescription(
            object : SdpObserver {
                override fun onSetSuccess() {
                    if (continuation.isActive) continuation.resume(Unit)
                }

                override fun onSetFailure(error: String?) {
                    if (continuation.isActive) {
                        continuation.resumeWithException(IllegalStateException(error ?: "setRemoteDescription failed"))
                    }
                }

                override fun onCreateSuccess(description: SessionDescription?) = Unit
                override fun onCreateFailure(error: String?) = Unit
            },
            description,
        )
    }

@Volatile
private var peerConnectionFactoryInitialized = false

internal fun ensurePeerConnectionFactory(context: Context) {
    if (peerConnectionFactoryInitialized) return
    synchronized(WebRtcCallMedia::class.java) {
        if (peerConnectionFactoryInitialized) return
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(context)
                .setEnableInternalTracer(false)
                .createInitializationOptions(),
        )
        peerConnectionFactoryInitialized = true
    }
}

internal fun buildPeerConnectionFactory(context: Context): PeerConnectionFactory {
    ensurePeerConnectionFactory(context)
    val audioModule = JavaAudioDeviceModule.builder(context)
        .setUseHardwareAcousticEchoCanceler(JavaAudioDeviceModule.isBuiltInAcousticEchoCancelerSupported())
        .setUseHardwareNoiseSuppressor(JavaAudioDeviceModule.isBuiltInNoiseSuppressorSupported())
        .setAudioAttributes(
            android.media.AudioAttributes.Builder()
                .setUsage(android.media.AudioAttributes.USAGE_VOICE_COMMUNICATION)
                .setContentType(android.media.AudioAttributes.CONTENT_TYPE_SPEECH)
                .build(),
        )
        .createAudioDeviceModule()
    return PeerConnectionFactory.builder()
        .setAudioDeviceModule(audioModule)
        .createPeerConnectionFactory()
}
