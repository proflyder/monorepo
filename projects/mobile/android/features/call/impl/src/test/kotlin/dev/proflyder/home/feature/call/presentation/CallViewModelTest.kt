package dev.proflyder.home.feature.call.presentation

import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.FakeCallAudio
import dev.proflyder.home.feature.call.FakeCallController
import dev.proflyder.home.feature.call.FakeClock
import dev.proflyder.home.feature.call.FakeCoordinator
import dev.proflyder.home.feature.call.FakePermissions
import dev.proflyder.home.feature.call.audio.CallAudioRoute
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
internal class CallViewModelTest {
    private val controller = FakeCallController()
    private val coordinator = FakeCoordinator()
    private val audio = FakeCallAudio()
    private val permissions = FakePermissions()
    private val clock = FakeClock()

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.viewModel(): CallViewModel {
        Dispatchers.setMain(UnconfinedTestDispatcher(testScheduler))
        return CallViewModel(controller, coordinator, audio, permissions, clock)
    }

    @Test
    fun openingWithStartAndPermissionsPlacesTheCallWithoutClosing() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(CallIntent.Open(startCall = true))
        runCurrent()
        assertEquals(1, coordinator.starts)
        controller.mutableState.value = CallSnapshot(phase = CallPhase.DIALING)
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ENDED, endReason = IntercomEndReason.HANGUP)
        viewModel.dispatch(CallIntent.HangupOrClose)
        assertEquals(CallEvent.Finish, viewModel.events.first())
    }

    @Test
    fun missingPermissionsAreRequestedAndDenialEndsTheCall() = runTest {
        permissions.missing = listOf("android.permission.RECORD_AUDIO")
        permissions.microphone = false
        val viewModel = viewModel()
        viewModel.dispatch(CallIntent.Open(startCall = true))
        assertEquals(CallEvent.RequestPermissions(permissions.missing), viewModel.events.first())
        viewModel.dispatch(CallIntent.PermissionsResult(granted = false))
        assertEquals(listOf("dismiss", "denied"), controller.actions)
        assertEquals(0, coordinator.starts)
    }

    @Test
    fun openingWithoutACallCloses() = runTest {
        val viewModel = viewModel()
        viewModel.dispatch(CallIntent.Open(startCall = false))
        runCurrent()
        assertEquals(CallEvent.Finish, viewModel.events.first())
    }

    @Test
    fun activeCallTicksElapsedTimeFromTheInjectedClock() = runTest {
        val since = clock.wall / 1_000 - 65
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ACTIVE, activeSinceUnix = since)
        val viewModel = viewModel()
        runCurrent()
        assertEquals(CallStatus.Active(65), viewModel.state.value.status)
        assertTrue(viewModel.state.value.proximityLock)
        clock.wall += 2_000
        advanceTimeBy(2_001)
        assertEquals(CallStatus.Active(67), viewModel.state.value.status)
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ENDED)
        runCurrent()
        assertFalse(viewModel.state.value.proximityLock)
    }

    @Test
    fun theScreenIsBlankedByProximityOnlyOnTheEarpiece() = runTest {
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ACTIVE, activeSinceUnix = clock.wall / 1_000)
        val viewModel = viewModel()
        runCurrent()
        assertTrue(viewModel.state.value.proximityLock)
        audio.state.value = audio.state.value.copy(selected = CallAudioRoute.SPEAKER)
        runCurrent()
        assertFalse(viewModel.state.value.proximityLock)
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ENDED)
        runCurrent()
    }

    @Test
    fun muteAndRoutesAreIntentsRenderedFromState() = runTest {
        controller.mutableState.value = CallSnapshot(phase = CallPhase.RINGING)
        val viewModel = viewModel()
        viewModel.dispatch(CallIntent.ToggleMute)
        assertEquals(listOf("muted=true"), controller.actions)
        viewModel.dispatch(CallIntent.ShowRoutes)
        runCurrent()
        assertTrue(viewModel.state.value.routeSheetVisible)
        viewModel.dispatch(CallIntent.SelectRoute(CallAudioRoute.SPEAKER))
        runCurrent()
        assertEquals(listOf(CallAudioRoute.SPEAKER), audio.selected)
        assertFalse(viewModel.state.value.routeSheetVisible)
    }

    @Test
    fun backDuringALiveCallMovesTheTaskBack() = runTest {
        controller.mutableState.value = CallSnapshot(phase = CallPhase.RINGING)
        val viewModel = viewModel()
        viewModel.dispatch(CallIntent.Back)
        assertEquals(CallEvent.MoveToBack, viewModel.events.first())
    }
}
