package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.feature.call.audio.CallAudioController
import dev.proflyder.home.feature.call.audio.CallAudioRoute
import dev.proflyder.home.feature.call.audio.CallAudioRouteState
import dev.proflyder.home.feature.call.presentation.CallPermissions
import dev.proflyder.home.feature.call.telecom.CallCoordinator
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

internal class FakeCallController(initial: CallSnapshot = CallSnapshot()) : CallController {
    val mutableState = MutableStateFlow(initial)
    override val state: StateFlow<CallSnapshot> = mutableState
    val actions = mutableListOf<String>()

    override fun placeCall() {
        actions += "place"
    }

    override fun hangup() {
        actions += "hangup"
    }

    override fun reportPermissionDenied() {
        actions += "denied"
    }

    override fun setMuted(muted: Boolean) {
        actions += "muted=$muted"
    }

    override fun dismissEnded() {
        actions += "dismiss"
        if (mutableState.value.phase == CallPhase.ENDED) mutableState.value = CallSnapshot()
    }
}

internal class FakeCallAudio : CallAudioController {
    override val state = MutableStateFlow(CallAudioRouteState())
    var starts = 0
    var stops = 0
    val selected = mutableListOf<CallAudioRoute>()

    override fun start() {
        starts++
    }

    override fun stop() {
        stops++
    }

    override fun refresh() = Unit

    override fun select(route: CallAudioRoute) {
        selected += route
    }
}

internal class FakeCoordinator : CallCoordinator {
    var starts = 0

    override fun startOutgoingCall() {
        starts++
    }
}

internal class FakePermissions(var missing: List<String> = emptyList(), var microphone: Boolean = true) : CallPermissions {
    override fun missing(): List<String> = missing

    override fun microphoneGranted(): Boolean = microphone
}

internal class FakeClock(var wall: Long = 1_795_200_000_000L) : Clock {
    override fun wallMillis(): Long = wall

    override fun elapsedNanos(): Long = 0L
}
