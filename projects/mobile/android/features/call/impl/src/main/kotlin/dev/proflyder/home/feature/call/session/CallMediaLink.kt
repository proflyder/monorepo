package dev.proflyder.home.feature.call.session

import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.presentation.LatestLauncher
import kotlinx.coroutines.CoroutineScope

internal class CallMediaLink(scope: CoroutineScope) {
    private val events = LatestLauncher(scope)
    private var media: CallMedia? = null
    private var offerSent = false

    fun attach(session: CallMedia, onEvent: suspend (CallMediaEvent) -> Unit) {
        media = session
        events.launch { session.events.collect(onEvent) }
    }

    suspend fun createOfferOnce(): Outcome<String>? {
        if (offerSent) return null
        offerSent = true
        return media?.createOffer()
    }

    suspend fun applyAnswer(sdp: String): Outcome<Unit>? = media?.setRemoteAnswer(sdp)

    suspend fun addCandidate(candidate: IntercomIceCandidate) {
        media?.addRemoteCandidate(candidate)
    }

    fun setMuted(muted: Boolean) {
        media?.setMuted(muted)
    }

    fun close() {
        events.cancel()
        media?.close()
        media = null
        offerSent = false
    }
}
