package dev.proflyder.home.feature.call.audio

import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.FakeCallAudio
import dev.proflyder.home.feature.call.FakeCallController
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
internal class CallAudioSessionTest {
    @Test
    fun audioRouteFollowsTheActivePhaseOnly() = runTest {
        val controller = FakeCallController()
        val audio = FakeCallAudio()
        CallAudioSession(controller, audio, backgroundScope).start()
        runCurrent()

        controller.mutableState.value = CallSnapshot(phase = CallPhase.CONNECTING)
        runCurrent()
        assertEquals(0, audio.starts)

        controller.mutableState.value = CallSnapshot(phase = CallPhase.ACTIVE, activeSinceUnix = 1)
        runCurrent()
        controller.mutableState.value = CallSnapshot(phase = CallPhase.ACTIVE, activeSinceUnix = 1, muted = true)
        runCurrent()
        assertEquals(1, audio.starts)
        val stopsWhileActive = audio.stops

        controller.mutableState.value = CallSnapshot(phase = CallPhase.ENDED)
        runCurrent()
        assertEquals(stopsWhileActive + 1, audio.stops)
    }
}
