package dev.proflyder.home.core.presentation

import android.os.Bundle
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.Lifecycle
import dev.proflyder.home.core.ui.ViewBinding
import kotlinx.serialization.builtins.serializer
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ScreenLifecycleTest {
    @Test
    fun screenWorkRunsOncePerInstanceWhileViewWorkRunsPerView() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, screen).commitNow()
        activity.supportFragmentManager.beginTransaction().detach(screen).commitNow()
        activity.supportFragmentManager.beginTransaction().attach(screen).commitNow()
        idle()

        screen.probe.dispatch(ProbeIntent.Announce("once"))
        idle()

        assertEquals(1, screen.screenCreates)
        assertEquals(2, screen.viewReadies)
        assertEquals(listOf("once"), screen.delivered)
        assertEquals(1, screen.resultListeners)
    }

    @Test
    fun anEventWaitsForResumedAndIsNotLostWhileWaiting() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        val manager = activity.supportFragmentManager
        manager.beginTransaction().add(android.R.id.content, screen).setMaxLifecycle(screen, Lifecycle.State.STARTED).commitNow()

        screen.probe.dispatch(ProbeIntent.Announce("dialog"))
        screen.probe.dispatch(ProbeIntent.Notice("notice"))
        idle()
        assertEquals(emptyList<String>(), screen.delivered)

        manager.beginTransaction().setMaxLifecycle(screen, Lifecycle.State.RESUMED).commitNow()
        idle()
        assertEquals(listOf("dialog", "notice"), screen.delivered)
    }

    @Test
    fun aStartedEventIsDeliveredBeforeResume() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, screen)
            .setMaxLifecycle(screen, Lifecycle.State.STARTED).commitNow()

        screen.probe.dispatch(ProbeIntent.Notice("notice"))
        idle()

        assertEquals(listOf("notice"), screen.delivered)
    }

    @Test
    fun aSliceIsNotRenderedAgainWhenAnotherFieldChanges() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, screen).commitNow()
        idle()

        screen.probe.dispatch(ProbeIntent.SetOther(7))
        idle()
        screen.probe.dispatch(ProbeIntent.SetTitle("next"))
        idle()

        assertEquals(listOf("start", "next"), screen.titles)
        assertEquals(3, screen.wholeRenders)
    }

    @Test
    fun visibilityFollowsTheViewAndCountsReturns() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        val manager = activity.supportFragmentManager
        manager.beginTransaction().add(android.R.id.content, screen).commitNow()
        assertEquals(true, screen.probe.visibility.visible.value)

        manager.beginTransaction().detach(screen).commitNow()
        assertEquals(false, screen.probe.visibility.visible.value)
        manager.beginTransaction().attach(screen).commitNow()
        assertEquals(true, screen.probe.visibility.visible.value)
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    data class ProbeState(val title: String = "start", val other: Int = 0)

    sealed interface ProbeIntent {
        data class Announce(val text: String) : ProbeIntent
        data class Notice(val text: String) : ProbeIntent
        data class SetTitle(val text: String) : ProbeIntent
        data class SetOther(val value: Int) : ProbeIntent
    }

    sealed interface ProbeEvent {
        val text: String

        data class Dialog(override val text: String) : ProbeEvent
        data class Notice(override val text: String) : ProbeEvent, DeliveredWhenStarted
    }

    class ProbeViewModel : StateViewModel<ProbeState, ProbeIntent, ProbeEvent>(ProbeState()) {
        override fun handle(intent: ProbeIntent) = when (intent) {
            is ProbeIntent.Announce -> emit(ProbeEvent.Dialog(intent.text))
            is ProbeIntent.Notice -> emit(ProbeEvent.Notice(intent.text))
            is ProbeIntent.SetTitle -> updateState { copy(title = intent.text) }
            is ProbeIntent.SetOther -> updateState { copy(other = intent.value) }
        }
    }

    class ProbeBinding(override val root: View) : ViewBinding

    class ProbeScreen : ScreenFragment<ProbeBinding, ProbeState, ProbeIntent, ProbeEvent>(::inflateProbe) {
        val probe = ProbeViewModel()
        override val viewModel: StateViewModel<ProbeState, ProbeIntent, ProbeEvent> get() = probe

        var screenCreates = 0
        var viewReadies = 0
        var resultListeners = 0
        var wholeRenders = 0
        val delivered = mutableListOf<String>()
        val titles = mutableListOf<String>()

        override fun onScreenCreate(savedInstanceState: Bundle?) {
            screenCreates += 1
            onResult(PickedTitle) { ProbeIntent.SetTitle(it) }
            resultListeners += 1
        }

        override fun onViewReady(binding: ProbeBinding, savedInstanceState: Bundle?) {
            viewReadies += 1
            render(ProbeState::title) { titles += it }
        }

        override fun render(state: ProbeState) {
            wholeRenders += 1
        }

        override fun onEvent(event: ProbeEvent) {
            delivered += event.text
        }
    }

    object PickedTitle : ResultKey<String>(String.serializer())

    object OtherTitle : ResultKey<String>(String.serializer())
}

private fun inflateProbe(inflater: LayoutInflater, parent: ViewGroup?, attach: Boolean): ScreenLifecycleTest.ProbeBinding =
    ScreenLifecycleTest.ProbeBinding(FrameLayout(inflater.context))
