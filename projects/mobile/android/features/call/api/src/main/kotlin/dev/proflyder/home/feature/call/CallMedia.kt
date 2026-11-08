package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.Flow

/** Local WebRTC events the signaling layer must forward to the peer. */
sealed interface CallMediaEvent {
    data class LocalCandidate(val candidate: IntercomIceCandidate) : CallMediaEvent

    /** ICE failed/closed; the call cannot continue. */
    data object Failed : CallMediaEvent
}

/**
 * Audio-only peer connection abstraction. The production implementation lives in
 * `features/call/impl` (libwebrtc); the state machine stays JVM-testable because
 * it only sees this interface.
 */
interface CallMedia {
    val events: Flow<CallMediaEvent>

    /** Creates the peer connection, microphone track and ICE servers. */
    suspend fun prepare(iceServers: List<IntercomIceServer>): Outcome<Unit>

    /** Creates the caller offer and applies it locally. */
    suspend fun createOffer(): Outcome<String>

    /** Applies the callee answer received over the signaling channel. */
    suspend fun setRemoteAnswer(sdp: String): Outcome<Unit>

    /** Trickles one remote ICE candidate. */
    suspend fun addRemoteCandidate(candidate: IntercomIceCandidate): Outcome<Unit>

    fun setMuted(muted: Boolean)

    /** Releases the microphone and the peer connection. Idempotent. */
    fun close()
}

interface CallMediaFactory {
    fun create(): CallMedia
}
