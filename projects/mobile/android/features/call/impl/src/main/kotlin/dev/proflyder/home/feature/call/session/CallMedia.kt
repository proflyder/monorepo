package dev.proflyder.home.feature.call.session

import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.Flow

internal sealed interface CallMediaEvent {
    data class LocalCandidate(val candidate: IntercomIceCandidate) : CallMediaEvent

    data object Failed : CallMediaEvent
}

internal interface CallMedia {
    val events: Flow<CallMediaEvent>

    suspend fun prepare(iceServers: List<IntercomIceServer>): Outcome<Unit>

    suspend fun createOffer(): Outcome<String>

    suspend fun setRemoteAnswer(sdp: String): Outcome<Unit>

    suspend fun addRemoteCandidate(candidate: IntercomIceCandidate): Outcome<Unit>

    fun setMuted(muted: Boolean)

    fun close()
}

internal interface CallMediaFactory {
    fun create(): CallMedia
}
