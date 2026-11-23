package dev.proflyder.home.core.presentation

import android.app.Application
import android.os.Bundle
import android.os.Looper
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import dev.proflyder.home.core.model.ScreenSectionKey
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = ScopesTestApp::class)
class ScreenScopesTest {
    @Test
    fun aSectionHasItsOwnViewModelAndRendersOnlyItsOwnState() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val host = HostScreen()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, host).commitNow()
        idle()
        val section = host.childFragmentManager.findFragmentByTag(ProbeSection.id) as SectionProbe
        assertEquals("0", section.text())

        host.probe.dispatch(HostIntent.Bump)
        idle()
        assertEquals(1, section.renders)

        section.viewModel.dispatch(CounterIntent.Bump)
        idle()
        assertEquals("1", section.text())
        assertEquals(2, section.renders)
        assertEquals(1, host.probe.state.value.count)
    }

    @Test
    fun anAttachedViewModelDeliversItsEventsToTheScreen() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val host = HostScreen()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, host).commitNow()
        host.extra.dispatch(CounterIntent.Announce)
        idle()
        assertEquals(listOf("extra"), host.events)
    }

    @Test
    fun aFlowViewModelIsSharedByTheFlowAndClearedWhenItsLastScreenLeaves() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val manager = activity.supportFragmentManager
        val list = FlowScreen()
        val editor = FlowScreen()
        manager.beginTransaction().add(android.R.id.content, list, "list").commitNow()
        manager.beginTransaction().add(android.R.id.content, editor, "editor").commitNow()
        val shared = list.shared
        assertSame(shared, editor.shared)

        manager.beginTransaction().remove(editor).commitNow()
        assertTrue(ViewModelProvider(activity)[FlowScopes::class.java].isActive(ProbeFlow))
        assertEquals(0, shared.cleared)

        manager.beginTransaction().remove(list).commitNow()
        assertEquals(1, shared.cleared)
        val again = FlowScreen()
        manager.beginTransaction().add(android.R.id.content, again, "again").commitNow()
        assertNotSame(shared, again.shared)
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()
}

class ScopesTestApp : Application(), ViewModelFactoryProvider, ScreenSectionsProvider {
    override val viewModelFactory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T =
            requireNotNull(modelClass.cast(if (modelClass == FlowProbeViewModel::class.java) FlowProbeViewModel() else CounterViewModel()))
    }

    override val screenSections: ScreenSections = ScreenSections(setOf(SectionEntry))

    private object SectionEntry : ScreenSection {
        override val key: ScreenSectionKey = ProbeSection

        override fun create(): Fragment = SectionProbe()
    }
}

object ProbeSection : ScreenSectionKey("probe/section")

object ProbeFlow : FlowKey("probe")

data class HostState(val count: Int = 0)

sealed interface HostIntent {
    data object Bump : HostIntent
}

class HostViewModel : StateViewModel<HostState, HostIntent, Unit>(HostState()) {
    override fun handle(intent: HostIntent) = updateState { copy(count = count + 1) }
}

data class CounterState(val count: Int = 0)

sealed interface CounterIntent {
    data object Bump : CounterIntent
    data object Announce : CounterIntent
}

open class CounterViewModel : StateViewModel<CounterState, CounterIntent, String>(CounterState()) {
    override fun handle(intent: CounterIntent) = when (intent) {
        CounterIntent.Bump -> updateState { copy(count = count + 1) }
        CounterIntent.Announce -> emit("extra")
    }
}

class FlowProbeViewModel : CounterViewModel() {
    var cleared = 0

    override fun onCleared() {
        cleared += 1
    }
}

class ProbeViewBinding(override val root: View) : ViewBinding

class HostScreen : ScreenFragment<ProbeViewBinding, HostState, HostIntent, Unit>(::inflateHost) {
    val probe = HostViewModel()
    val extra = CounterViewModel()
    val events = mutableListOf<String>()
    override val viewModel: StateViewModel<HostState, HostIntent, Unit> get() = probe

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        attach(extra) { events += it }
        section(SLOT_ID, ProbeSection)
    }

    companion object {
        const val SLOT_ID = 0x7f0a1001
    }
}

class SectionProbe : SectionFragment<ProbeViewBinding, CounterState, CounterIntent, String>(::inflateSection) {
    public override val viewModel: CounterViewModel by screenViewModel()
    var renders = 0

    fun text(): String = (binding.root as TextView).text.toString()

    override fun onViewReady(binding: ProbeViewBinding, savedInstanceState: Bundle?) {
        render(CounterState::count) { count ->
            renders += 1
            (binding.root as TextView).text = count.toString()
        }
    }
}

class FlowScreen : Fragment(), FlowMember {
    override val flow: FlowKey = ProbeFlow

    private val model: FlowProbeViewModel by flowViewModel()

    lateinit var shared: FlowProbeViewModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        shared = model
    }
}

private fun inflateHost(inflater: LayoutInflater, parent: ViewGroup?, attach: Boolean): ProbeViewBinding =
    ProbeViewBinding(FrameLayout(inflater.context).apply { id = HostScreen.SLOT_ID })

private fun inflateSection(inflater: LayoutInflater, parent: ViewGroup?, attach: Boolean): ProbeViewBinding =
    ProbeViewBinding(TextView(inflater.context))
