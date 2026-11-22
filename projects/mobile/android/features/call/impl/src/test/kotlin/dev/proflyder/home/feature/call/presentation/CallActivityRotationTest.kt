package dev.proflyder.home.feature.call.presentation

import android.os.Looper
import android.widget.TextView
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.CallTestApplication
import dev.proflyder.home.feature.call.FakeCallAudio
import dev.proflyder.home.feature.call.FakeCallController
import dev.proflyder.home.feature.call.FakeClock
import dev.proflyder.home.feature.call.FakeCoordinator
import dev.proflyder.home.feature.call.FakePermissions
import dev.proflyder.home.feature.call.audio.CallAudioSession
import dev.proflyder.home.feature.call.impl.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = CallTestApplication::class)
internal class CallActivityRotationTest {
    private val clock = FakeClock()
    private val controller = FakeCallController(
        CallSnapshot(phase = CallPhase.ACTIVE, activeSinceUnix = clock.wall / 1_000 - 5),
    )
    private val audio = FakeCallAudio()
    private val scope = CoroutineScope(Dispatchers.Unconfined)

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun rotationDuringACallKeepsTheAudioRoute() {
        CallTestApplication.factory = SimpleViewModelFactory {
            CallViewModel(controller, FakeCoordinator(), audio, FakePermissions(), clock)
        }
        CallAudioSession(controller, audio, scope).start()
        val stopsBefore = audio.stops

        val activity = Robolectric.buildActivity(CallActivity::class.java).setup()
        shadowOf(Looper.getMainLooper()).idle()
        activity.recreate()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, audio.starts)
        assertEquals(stopsBefore, audio.stops)
        val status = activity.get().findViewById<TextView>(R.id.call_status).text.toString()
        assertEquals(activity.get().getString(R.string.call_status_active_timer, "00:05"), status)
        assertFalse(activity.get().isFinishing)
        activity.pause().stop().destroy()
        assertEquals(stopsBefore, audio.stops)
    }
}
