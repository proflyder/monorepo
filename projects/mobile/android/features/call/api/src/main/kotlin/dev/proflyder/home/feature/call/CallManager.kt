package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.IntercomCallState
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomEndpointKind
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomSdpType
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.IntercomClient
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * The phone-side call state machine.
 *
 * It owns exactly one live call at a time: `StartCall` -> `RINGING` ->
 * `CONNECTING` (offer/answer) -> `ACTIVE` -> `ENDED`, driven by the intercom
 * `Watch` event stream. Media (libwebrtc) and signaling (Connect) are behind
 * interfaces, so the whole transition set runs as a plain JVM test.
 *
 * The manager is app-scoped and keeps the `Watch` stream open permanently; the
 * server marks the phone online while that stream exists.
 */
class CallManager(
    private val client: IntercomClient,
    private val mediaFactory: CallMediaFactory,
    private val scope: CoroutineScope,
    private val nowMillis: () -> Long = System::currentTimeMillis,
    private val newIdempotencyKey: () -> String = { UUID.randomUUID().toString() },
    private val watchdogMillis: Long = DEFAULT_WATCHDOG_MILLIS,
    private val peerFallbackName: String = "",
) : CallController {

    private val mutableState = MutableStateFlow(CallSnapshot())
    override val state: StateFlow<CallSnapshot> = mutableState.asStateFlow()

    private val mutex = Mutex()
    private var media: CallMedia? = null
    private var mediaJob: Job? = null
    private var watchdog: Job? = null
    private var selfEndpointId: String? = null
    private var offerSent = false
    /** True once this phone ended the call, so a late server event cannot add a failure. */
    private var endedLocally = false

    init {
        scope.launch {
            client.watch().collect { event -> onWatchEvent(event) }
        }
    }

    override fun placeCall() {
        scope.launch {
            mutex.withLock {
                if (state.value.isLive) return@withLock
                resetForNewCall()
                dial()
            }
        }
    }

    override fun hangup() {
        val snapshot = state.value
        val wasRinging = snapshot.phase == CallPhase.DIALING || snapshot.phase == CallPhase.RINGING
        finishLocally(
            reason = if (wasRinging) IntercomEndReason.CANCELED else IntercomEndReason.HANGUP,
            failure = null,
        )
    }

    override fun reportPermissionDenied() {
        mutableState.value = CallSnapshot(
            phase = CallPhase.ENDED,
            failure = CallFailure.PermissionDenied,
        )
    }

    override fun setMuted(muted: Boolean) {
        mutableState.update { it.copy(muted = muted) }
        media?.setMuted(muted)
    }

    override fun dismissEnded() {
        if (state.value.phase == CallPhase.ENDED) {
            mutableState.value = CallSnapshot()
        }
    }

    override fun acceptIncoming(callId: String) {
        // TODO(incoming): no incoming-call screen. Keeping the method makes the
        // contract symmetric for a future ring UI.
    }

    override fun declineIncoming(callId: String) {
        scope.launch { client.hangup(callId, IntercomEndReason.DECLINED) }
    }

    private suspend fun dial() {
        mutableState.value = CallSnapshot(phase = CallPhase.DIALING, peerName = peerFallbackName)

        val station = when (val endpoints = client.listEndpoints()) {
            is Outcome.Failure -> {
                finishLocally(reason = IntercomEndReason.UNREACHABLE, failure = endpoints.failure.toCallFailure(stationLookup = true))
                return
            }
            is Outcome.Success -> endpoints.value.firstOrNull {
                it.kind == IntercomEndpointKind.STATION && it.online
            } ?: endpoints.value.firstOrNull { it.kind == IntercomEndpointKind.STATION }
        }
        if (station == null) {
            finishLocally(reason = IntercomEndReason.UNREACHABLE, failure = CallFailure.StationOffline)
            return
        }
        if (state.value.phase == CallPhase.ENDED) return
        updatePeer(station)

        // Registering is optional (Watch marks us online) but gives us the
        // server-derived endpoint identity used to recognise our own events.
        val registration = client.registerEndpoint("Phone", listOf("audio"))
        if (registration is Outcome.Success) selfEndpointId = registration.value.id

        if (state.value.phase == CallPhase.ENDED) return
        val started = when (val result = client.startCall(station.id, newIdempotencyKey())) {
            is Outcome.Failure -> {
                finishLocally(result.failure.toEndReason(), result.failure.toCallFailure())
                return
            }
            is Outcome.Success -> result.value
        }
        if (state.value.phase == CallPhase.ENDED) {
            // The user ended the call while StartCall was in flight.
            client.hangup(started.id, IntercomEndReason.CANCELED)
            return
        }
        mutableState.update {
            it.copy(callId = started.id, phase = started.state.toPhase(CallPhase.RINGING))
        }
        startWatchdog()

        // Media starts only after the server accepted the call, so a busy or
        // offline station never opens the microphone.
        val iceServers = when (val config = client.getIceConfig()) {
            is Outcome.Success -> config.value
            is Outcome.Failure -> {
                // Empty ICE config means host candidates only; a failure to read
                // it is not fatal for the LAN/VPN case.
                emptyList()
            }
        }
        val mediaSession = mediaFactory.create()
        media = mediaSession
        when (mediaSession.prepare(iceServers)) {
            is Outcome.Failure -> {
                finishLocally(reason = IntercomEndReason.ERROR, failure = CallFailure.MediaFailure)
                return
            }
            is Outcome.Success -> Unit
        }
        if (state.value.phase == CallPhase.ENDED) return
        mediaSession.setMuted(state.value.muted)
        mediaJob = scope.launch {
            mediaSession.events.collect { event ->
                when (event) {
                    is CallMediaEvent.LocalCandidate -> sendSignal(IntercomOutgoingSignal.IceCandidate(event.candidate))
                    CallMediaEvent.Failed -> finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
                }
            }
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
            // TODO(incoming): a future ring screen accepts here. Today the phone
            // declines any incoming call and stays IDLE.
            if (call.state == IntercomCallState.RINGING &&
                selfEndpointId != null &&
                call.callee?.id == selfEndpointId
            ) {
                client.hangup(call.id, IntercomEndReason.DECLINED)
            }
            return
        }

        if (current.phase == CallPhase.ENDED) return
        if (current.callId == null) mutableState.update { it.copy(callId = call.id) }
        call.callee?.let(::updatePeer)

        when (call.state) {
            IntercomCallState.RINGING -> updatePhase(CallPhase.RINGING)
            IntercomCallState.CONNECTING -> {
                updatePhase(CallPhase.CONNECTING)
                sendOfferOnce()
            }
            IntercomCallState.ACTIVE -> {
                watchdog?.cancel()
                mutableState.update {
                    it.copy(
                        phase = CallPhase.ACTIVE,
                        activeSinceUnix = if (it.activeSinceUnix > 0) it.activeSinceUnix else nowMillis() / 1000,
                    )
                }
            }
            IntercomCallState.ENDED -> {
                if (!endedLocally) {
                    finishLocally(call.endReason, null, alreadyEnded = true)
                }
            }
            IntercomCallState.UNSPECIFIED -> Unit
        }
    }

    private suspend fun onSdp(event: IntercomWatchEvent.Sdp) {
        val current = state.value
        if (current.callId != event.callId) return
        if (event.signal.type != IntercomSdpType.ANSWER) return
        val session = media ?: return
        when (session.setRemoteAnswer(event.signal.sdp)) {
            is Outcome.Failure -> finishLocally(IntercomEndReason.ERROR, CallFailure.MediaFailure)
            is Outcome.Success -> updatePhase(CallPhase.CONNECTING)
        }
    }

    private suspend fun onRemoteCandidate(event: IntercomWatchEvent.IceCandidate) {
        if (state.value.callId != event.callId) return
        media?.addRemoteCandidate(event.candidate)
    }

    private suspend fun sendOfferOnce() {
        if (offerSent) return
        offerSent = true
        val session = media ?: return
        when (val offer = session.createOffer()) {
            is Outcome.Success -> sendSignal(
                IntercomOutgoingSignal.Sdp(
                    dev.proflyder.home.core.model.IntercomSdpSignal(IntercomSdpType.OFFER, offer.value),
                ),
            )
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

    private fun startWatchdog() {
        watchdog?.cancel()
        watchdog = scope.launch {
            delay(watchdogMillis)
            if (state.value.phase != CallPhase.ACTIVE && state.value.phase != CallPhase.ENDED) {
                finishLocally(IntercomEndReason.TIMEOUT, CallFailure.Timeout)
            }
        }
    }

    private fun finishLocally(
        reason: IntercomEndReason,
        failure: CallFailure?,
        alreadyEnded: Boolean = false,
    ) {
        val id = state.value.callId
        if (!alreadyEnded && id != null) {
            val reported = when (reason) {
                IntercomEndReason.CANCELED -> IntercomEndReason.CANCELED
                IntercomEndReason.DECLINED -> IntercomEndReason.DECLINED
                else -> IntercomEndReason.HANGUP
            }
            scope.launch { client.hangup(id, reported) }
        }
        watchdog?.cancel()
        watchdog = null
        mediaJob?.cancel()
        mediaJob = null
        media?.close()
        media = null
        offerSent = false
        endedLocally = !alreadyEnded
        mutableState.update {
            it.copy(
                phase = CallPhase.ENDED,
                endReason = reason,
                failure = failure,
            )
        }
    }

    private fun resetForNewCall() {
        watchdog?.cancel()
        mediaJob?.cancel()
        media?.close()
        media = null
        mediaJob = null
        watchdog = null
        offerSent = false
        endedLocally = false
        mutableState.value = CallSnapshot()
    }

    private fun IntercomCallState.toPhase(fallback: CallPhase): CallPhase = when (this) {
        IntercomCallState.RINGING -> CallPhase.RINGING
        IntercomCallState.CONNECTING -> CallPhase.CONNECTING
        IntercomCallState.ACTIVE -> CallPhase.ACTIVE
        IntercomCallState.ENDED -> CallPhase.ENDED
        IntercomCallState.UNSPECIFIED -> fallback
    }

    private fun RemoteFailure.toCallFailure(stationLookup: Boolean = false): CallFailure = when (this) {
        RemoteFailure.NotConfigured -> CallFailure.NotConfigured
        RemoteFailure.Unauthorized -> CallFailure.Remote(this)
        is RemoteFailure.Validation -> CallFailure.NotConfigured
        is RemoteFailure.Offline -> if (stationLookup) CallFailure.StationOffline else CallFailure.Remote(this)
        RemoteFailure.Timeout -> CallFailure.Timeout
        is RemoteFailure.Tls -> CallFailure.Remote(this)
        is RemoteFailure.Server -> if (code == "failed_precondition") CallFailure.Busy else CallFailure.Remote(this)
        is RemoteFailure.ProtocolFailure -> CallFailure.Remote(this)
        is RemoteFailure.Storage -> CallFailure.Remote(this)
    }

    private fun RemoteFailure.toEndReason(): IntercomEndReason = when {
        this is RemoteFailure.Server && code == "failed_precondition" -> IntercomEndReason.BUSY
        this is RemoteFailure.Offline -> IntercomEndReason.UNREACHABLE
        this == RemoteFailure.Timeout -> IntercomEndReason.TIMEOUT
        else -> IntercomEndReason.ERROR
    }

    private companion object {
        const val DEFAULT_WATCHDOG_MILLIS = 60_000L
    }
}
