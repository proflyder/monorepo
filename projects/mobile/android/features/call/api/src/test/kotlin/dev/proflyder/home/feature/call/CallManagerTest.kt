package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.IntercomCall
import dev.proflyder.home.core.model.IntercomCallState
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.IntercomEndpoint
import dev.proflyder.home.core.model.IntercomEndpointKind
import dev.proflyder.home.core.model.IntercomIceCandidate
import dev.proflyder.home.core.model.IntercomIceServer
import dev.proflyder.home.core.model.IntercomOutgoingSignal
import dev.proflyder.home.core.model.IntercomSdpSignal
import dev.proflyder.home.core.model.IntercomSdpType
import dev.proflyder.home.core.model.IntercomStatus
import dev.proflyder.home.core.model.IntercomWatchEvent
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.IntercomClient
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CallManagerTest {

    private val station = IntercomEndpoint(
        id = "station",
        kind = IntercomEndpointKind.STATION,
        displayName = "Station",
        online = true,
    )

    private fun call(state: IntercomCallState, reason: IntercomEndReason = IntercomEndReason.UNSPECIFIED) =
        IntercomCall(
            id = "call-1",
            caller = IntercomEndpoint("phone", IntercomEndpointKind.PHONE, "Phone", online = true),
            callee = station,
            state = state,
            endReason = reason,
        )

    private class FakeClient : IntercomClient {
        var endpoints: Outcome<List<IntercomEndpoint>> = Outcome.Success(emptyList())
        var startResult: Outcome<IntercomCall> = Outcome.Failure(RemoteFailure.ProtocolFailure("unset"))
        var iceServers: Outcome<List<IntercomIceServer>> = Outcome.Success(emptyList())
        val events = MutableSharedFlow<IntercomWatchEvent>(extraBufferCapacity = 32)
        val signals = mutableListOf<IntercomOutgoingSignal>()
        var hungUp: Pair<String, IntercomEndReason>? = null
        var startGate: kotlinx.coroutines.CompletableDeferred<Unit>? = null

        override suspend fun registerEndpoint(
            displayName: String,
            capabilities: List<String>,
        ): Outcome<IntercomEndpoint> =
            Outcome.Success(IntercomEndpoint("phone", IntercomEndpointKind.PHONE, displayName, online = true))

        override suspend fun listEndpoints(): Outcome<List<IntercomEndpoint>> = endpoints

        override suspend fun startCall(calleeEndpointId: String, idempotencyKey: String): Outcome<IntercomCall> {
            startGate?.await()
            return startResult
        }

        override suspend fun answer(callId: String): Outcome<IntercomCall> = startResult

        override suspend fun hangup(callId: String, reason: IntercomEndReason): Outcome<IntercomCall> {
            hungUp = callId to reason
            return Outcome.Success(IntercomCall(id = callId, state = IntercomCallState.ENDED))
        }

        override suspend fun sendSignal(callId: String, signal: IntercomOutgoingSignal): Outcome<Boolean> {
            signals += signal
            return Outcome.Success(true)
        }

        override suspend fun getIceConfig(): Outcome<List<IntercomIceServer>> = iceServers

        override suspend fun getStatus(): Outcome<IntercomStatus> = Outcome.Success(IntercomStatus())

        override fun watch(): Flow<IntercomWatchEvent> = events
    }

    private class FakeMedia : CallMedia {
        val mutableEvents = MutableSharedFlow<CallMediaEvent>(extraBufferCapacity = 16)
        override val events: Flow<CallMediaEvent> get() = mutableEvents
        var offer: Outcome<String> = Outcome.Success("offer-sdp")
        var answerApplied: String? = null
        val candidates = mutableListOf<IntercomIceCandidate>()
        var muted: Boolean? = null
        var closed = false

        override suspend fun prepare(iceServers: List<IntercomIceServer>): Outcome<Unit> = Outcome.Success(Unit)
        override suspend fun createOffer(): Outcome<String> = offer
        override suspend fun setRemoteAnswer(sdp: String): Outcome<Unit> {
            answerApplied = sdp
            return Outcome.Success(Unit)
        }
        override suspend fun addRemoteCandidate(candidate: IntercomIceCandidate): Outcome<Unit> {
            candidates += candidate
            return Outcome.Success(Unit)
        }
        override fun setMuted(muted: Boolean) {
            this.muted = muted
        }
        override fun close() {
            closed = true
        }
    }

    private class FakeFactory(private val media: FakeMedia) : CallMediaFactory {
        override fun create(): CallMedia = media
    }

    private fun TestScope.manager(
        client: FakeClient,
        media: FakeMedia,
        watchdogMillis: Long = 60_000,
    ): CallManager = CallManager(
        client = client,
        mediaFactory = FakeFactory(media),
        scope = backgroundScope,
        watchdogMillis = watchdogMillis,
        newIdempotencyKey = { "key-1" },
        peerFallbackName = "Station",
    )

    @Test
    fun happyPathRingsConnectsAndBecomesActive() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val media = FakeMedia()
        val manager = manager(client, media)

        manager.placeCall()
        runCurrent()
        assertEquals(CallPhase.RINGING, manager.state.value.phase)
        assertEquals("call-1", manager.state.value.callId)
        assertEquals("Station", manager.state.value.peerName)

        client.events.emit(IntercomWatchEvent.CallChanged(call(IntercomCallState.CONNECTING), IntercomCallState.RINGING))
        runCurrent()
        assertEquals(CallPhase.CONNECTING, manager.state.value.phase)
        val offer = client.signals.filterIsInstance<IntercomOutgoingSignal.Sdp>().single()
        assertEquals(IntercomSdpType.OFFER, offer.signal.type)
        assertEquals("offer-sdp", offer.signal.sdp)

        client.events.emit(
            IntercomWatchEvent.Sdp(
                "call-1",
                "station",
                IntercomSdpSignal(IntercomSdpType.ANSWER, "answer-sdp"),
            ),
        )
        runCurrent()
        assertEquals("answer-sdp", media.answerApplied)

        client.events.emit(IntercomWatchEvent.CallChanged(call(IntercomCallState.ACTIVE), IntercomCallState.CONNECTING))
        runCurrent()
        assertEquals(CallPhase.ACTIVE, manager.state.value.phase)
        assertTrue(manager.state.value.activeSinceUnix > 0)
    }

    @Test
    fun localHangupEndsAsCanceledWhileRinging() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()

        manager.hangup()
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(IntercomEndReason.CANCELED, manager.state.value.endReason)
        assertEquals("call-1" to IntercomEndReason.CANCELED, client.hungUp)
    }

    @Test
    fun remoteHangupEndsWithServerReason() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()

        client.events.emit(
            IntercomWatchEvent.CallChanged(
                call(IntercomCallState.ENDED, IntercomEndReason.DECLINED),
                IntercomCallState.RINGING,
            ),
        )
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(IntercomEndReason.DECLINED, manager.state.value.endReason)
        assertNull(manager.state.value.failure)
    }

    @Test
    fun mediaFailureEndsTheCall() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val media = FakeMedia()
        val manager = manager(client, media)
        manager.placeCall()
        runCurrent()

        media.mutableEvents.emit(CallMediaEvent.Failed)
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.MediaFailure, manager.state.value.failure)
        assertTrue(media.closed)
    }

    @Test
    fun iceFailureBeforeActiveIsReported() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.CONNECTING))
        }
        val media = FakeMedia()
        val manager = manager(client, media)
        manager.placeCall()
        runCurrent()

        media.mutableEvents.emit(CallMediaEvent.Failed)
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.MediaFailure, manager.state.value.failure)
    }

    @Test
    fun noAnswerTimesOut() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val manager = manager(client, FakeMedia(), watchdogMillis = 1_000)
        manager.placeCall()
        runCurrent()

        advanceTimeBy(1_001)
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.Timeout, manager.state.value.failure)
        assertEquals(IntercomEndReason.TIMEOUT, manager.state.value.endReason)
    }

    @Test
    fun permissionDeniedIsAnExplicitEndedState() = runTest {
        val manager = manager(FakeClient().apply { endpoints = Outcome.Success(emptyList()) }, FakeMedia())
        manager.reportPermissionDenied()
        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.PermissionDenied, manager.state.value.failure)
    }

    @Test
    fun offlineStationIsReportedBeforeStartCall() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Failure(RemoteFailure.Offline("no route"))
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.StationOffline, manager.state.value.failure)
        assertNull(client.hungUp)
    }

    @Test
    fun busyCallMapsFailedPrecondition() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Failure(RemoteFailure.Server("failed_precondition", "busy"))
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals(CallFailure.Busy, manager.state.value.failure)
        assertEquals(IntercomEndReason.BUSY, manager.state.value.endReason)
    }

    @Test
    fun muteIsReflectedInStateAndMedia() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val media = FakeMedia()
        val manager = manager(client, media)
        manager.placeCall()
        runCurrent()

        manager.setMuted(true)
        assertTrue(manager.state.value.muted)
        assertEquals(true, media.muted)

        manager.setMuted(false)
        assertFalse(manager.state.value.muted)
        assertEquals(false, media.muted)
    }

    @Test
    fun hangupWhileStartCallIsInFlightDoesNotResurrectTheCall() = runTest {
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
            startGate = gate
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()
        assertEquals(CallPhase.DIALING, manager.state.value.phase)

        manager.hangup()
        runCurrent()
        gate.complete(Unit)
        runCurrent()

        assertEquals(CallPhase.ENDED, manager.state.value.phase)
        assertEquals("call-1" to IntercomEndReason.CANCELED, client.hungUp)
    }

    @Test
    fun dismissEndedReturnsToIdle() = runTest {
        val client = FakeClient().apply {
            endpoints = Outcome.Success(listOf(station))
            startResult = Outcome.Success(call(IntercomCallState.RINGING))
        }
        val manager = manager(client, FakeMedia())
        manager.placeCall()
        runCurrent()
        manager.hangup()
        runCurrent()
        assertEquals(CallPhase.ENDED, manager.state.value.phase)

        manager.dismissEnded()
        assertEquals(CallPhase.IDLE, manager.state.value.phase)
        assertNull(manager.state.value.callId)
    }
}
