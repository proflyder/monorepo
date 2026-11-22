package dev.proflyder.home.feature.call.session

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.IntercomCallState
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomEndpointKind
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomSdpSignal
import dev.proflyder.home.core.model.IntercomSdpType
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.IntercomClient
import dev.proflyder.home.core.presentation.LatestLauncher
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

@SingleIn(AppScope::class)
@Inject
internal class CallManager(
    private val client: IntercomClient,
    private val mediaFactory: CallMediaFactory,
    private val scope: CoroutineScope,
    private val clock: Clock,
    private val settings: CallSessionSettings,
) : CallController {

    private val mutableState = MutableStateFlow(CallSnapshot())
    override val state: StateFlow<CallSnapshot> = mutableState.asStateFlow()

    private val mutex = Mutex()
    private val media = CallMediaLink(scope)
    private val watch = LatestLauncher(scope)
    private val watchdog = LatestLauncher(scope)
    private var selfEndpointId: String? = null
    private var endedLocally = false

    override fun placeCall() {
        scope.launch { mutex.withLock { startSession() } }
    }

    private suspend fun startSession() {
        if (state.value.isLive) return
        resetForNewCall()
        watch.launch { client.watch().collect(::onWatchEvent) }
        dial()
    }

    override fun hangup() {
        val phase = state.value.phase
        val wasRinging = phase == CallPhase.DIALING || phase == CallPhase.RINGING
        finishLocally(if (wasRinging) IntercomEndReason.CANCELED else IntercomEndReason.HANGUP, failure = null)
    }

    override fun reportPermissionDenied() {
        mutableState.value = CallSnapshot(phase = CallPhase.ENDED, failure = CallFailure.PermissionDenied)
    }

    override fun setMuted(muted: Boolean) {
        mutableState.update { it.copy(muted = muted) }
        media.setMuted(muted)
    }

    override fun dismissEnded() {
        mutableState.update { if (it.phase == CallPhase.ENDED) CallSnapshot() else it }
    }

    private suspend fun dial() {
        mutableState.value = CallSnapshot(phase = CallPhase.DIALING, peerName = settings.peerFallbackName)
        val station = findStation() ?: return
        if (isEnded()) return
        updatePeer(station)
        val registration = client.registerEndpoint(SELF_NAME, SELF_CAPABILITIES)
        if (registration is Outcome.Success) selfEndpointId = registration.value.id
        if (isEnded()) return
        val callId = startCall(station.id) ?: return
        startWatchdog()
        connectMedia(callId)
    }

    private suspend fun findStation(): IntercomEndpoint? {
        val station = when (val endpoints = client.listEndpoints()) {
            is Outcome.Failure -> {
                finishLocally(IntercomEndReason.UNREACHABLE, endpoints.failure.toCallFailure(stationLookup = true))
                return null
            }
            is Outcome.Success -> endpoints.value.preferredStation()
        }
        if (station == null) finishLocally(IntercomEndReason.UNREACHABLE, CallFailure.StationOffline)
        return station
    }

    private suspend fun startCall(stationId: String): String? {
        val started = when (val result = client.startCall(stationId, settings.newIdempotencyKey())) {
            is Outcome.Failure -> {
                finishLocally(result.failure.toEndReason(), result.failure.toCallFailure())
                return null
            }
            is Outcome.Success -> result.value
        }
        if (isEnded()) {
            client.hangup(started.id, IntercomEndReason.CANCELED)
            return null
        }
        mutableState.update { it.copy(callId = started.id, phase = started.state.toPhase(CallPhase.RINGING)) }
        return started.id
    }

    private suspend fun connectMedia(callId: String) {
        val iceServers = (client.getIceConfig() as? Outcome.Success)?.value.orEmpty()
        val session = mediaFactory.create()
        if (session.prepare(iceServers) is Outcome.Failure) {
            session.close()
            finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
            return
        }
        if (isEnded() || state.value.callId != callId) {
            session.close()
            return
        }
        session.setMuted(state.value.muted)
        media.attach(session, ::onMediaEvent)
    }

    private suspend fun onMediaEvent(event: CallMediaEvent) {
        when (event) {
            is CallMediaEvent.LocalCandidate -> sendSignal(IntercomOutgoingSignal.IceCandidate(event.candidate))
            CallMediaEvent.Failed -> finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
        }
    }

    private suspend fun onWatchEvent(event: IntercomWatchEvent) {
        mutex.withLock {
            when (event) {
                is IntercomWatchEvent.CallChanged -> onCallChanged(event)
                is IntercomWatchEvent.Sdp -> onSdp(event)
                is IntercomWatchEvent.IceCandidate -> onRemoteCandidate(event)
                is IntercomWatchEvent.IceEnd -> Unit
            }
        }
    }

    private suspend fun onCallChanged(event: IntercomWatchEvent.CallChanged) {
        val call = event.call
        val current = state.value
        val mine = current.callId == call.id ||
            (current.callId == null && selfEndpointId != null && call.caller?.id == selfEndpointId)
        if (!mine) {
            declineIfRingingSelf(event)
            return
        }
        if (current.phase == CallPhase.ENDED) return
        if (current.callId == null) mutableState.update { it.copy(callId = call.id) }
        call.callee?.let(::updatePeer)
        applyRemoteState(call.state, call.endReason)
    }

    private suspend fun declineIfRingingSelf(event: IntercomWatchEvent.CallChanged) {
        val call = event.call
        val self = selfEndpointId ?: return
        if (call.state == IntercomCallState.RINGING && call.callee?.id == self) {
            client.hangup(call.id, IntercomEndReason.DECLINED)
        }
    }

    private suspend fun applyRemoteState(remote: IntercomCallState, endReason: IntercomEndReason) {
        when (remote) {
            IntercomCallState.RINGING -> updatePhase(CallPhase.RINGING)
            IntercomCallState.CONNECTING -> {
                updatePhase(CallPhase.CONNECTING)
                sendOfferOnce()
            }
            IntercomCallState.ACTIVE -> markActive()
            IntercomCallState.ENDED -> if (!endedLocally) finishLocally(endReason, null, alreadyEnded = true)
            IntercomCallState.UNSPECIFIED -> Unit
        }
    }

    private fun markActive() {
        watchdog.cancel()
        val nowUnix = clock.wallMillis() / MILLIS_PER_SECOND
        mutableState.update {
            it.copy(phase = CallPhase.ACTIVE, activeSinceUnix = it.activeSinceUnix.takeIf { since -> since > 0 } ?: nowUnix)
        }
    }

    private suspend fun onSdp(event: IntercomWatchEvent.Sdp) {
        if (state.value.callId != event.callId) return
        if (event.signal.type != IntercomSdpType.ANSWER) return
        when (media.applyAnswer(event.signal.sdp)) {
            null -> Unit
            is Outcome.Failure -> finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
            is Outcome.Success -> updatePhase(CallPhase.CONNECTING)
        }
    }

    private suspend fun onRemoteCandidate(event: IntercomWatchEvent.IceCandidate) {
        if (state.value.callId != event.callId) return
        media.addCandidate(event.candidate)
    }

    private suspend fun sendOfferOnce() {
        when (val offer = media.createOfferOnce()) {
            null -> Unit
            is Outcome.Success -> sendSignal(IntercomOutgoingSignal.Sdp(IntercomSdpSignal(IntercomSdpType.OFFER, offer.value)))
            is Outcome.Failure -> finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
        }
    }

    private suspend fun sendSignal(signal: IntercomOutgoingSignal) {
        val id = state.value.callId ?: return
        client.sendSignal(id, signal)
    }

    private fun updatePeer(endpoint: IntercomEndpoint) {
        if (endpoint.displayName.isBlank()) return
        mutableState.update { if (it.peerName.isBlank()) it.copy(peerName = endpoint.displayName) else it }
    }

    private fun updatePhase(phase: CallPhase) {
        mutableState.update { if (it.phase == CallPhase.ENDED) it else it.copy(phase = phase) }
    }

    private fun isEnded(): Boolean = state.value.phase == CallPhase.ENDED

    private fun startWatchdog() {
        watchdog.launch {
            delay(settings.watchdog)
            if (state.value.phase != CallPhase.ACTIVE && !isEnded()) {
                finishLocally(IntercomEndReason.TIMEOUT, CallFailure.Timeout)
            }
        }
    }

    private fun finishLocally(reason: IntercomEndReason, failure: CallFailure?, alreadyEnded: Boolean = false) {
        val id = state.value.callId
        if (!alreadyEnded && id != null) {
            scope.launch { client.hangup(id, reason.reported()) }
        }
        releaseSession()
        endedLocally = !alreadyEnded
        mutableState.update { it.copy(phase = CallPhase.ENDED, endReason = reason, failure = failure) }
    }

    private fun resetForNewCall() {
        releaseSession()
        endedLocally = false
        mutableState.value = CallSnapshot()
    }

    private fun releaseSession() {
        watchdog.cancel()
        media.close()
        watch.cancel()
    }

    private companion object {
        const val MILLIS_PER_SECOND = 1_000L
        const val SELF_NAME = "Phone"
        val SELF_CAPABILITIES = listOf("audio")
    }
}

internal fun List<IntercomEndpoint>.preferredStation(): IntercomEndpoint? =
    firstOrNull { it.kind == IntercomEndpointKind.STATION && it.online }
        ?: firstOrNull { it.kind == IntercomEndpointKind.STATION }

internal fun IntercomEndReason.reported(): IntercomEndReason = when (this) {
    IntercomEndReason.CANCELED, IntercomEndReason.DECLINED -> this
    else -> IntercomEndReason.HANGUP
}

internal fun IntercomCallState.toPhase(fallback: CallPhase): CallPhase = when (this) {
    IntercomCallState.RINGING -> CallPhase.RINGING
    IntercomCallState.CONNECTING -> CallPhase.CONNECTING
    IntercomCallState.ACTIVE -> CallPhase.ACTIVE
    IntercomCallState.ENDED -> CallPhase.ENDED
    IntercomCallState.UNSPECIFIED -> fallback
}
