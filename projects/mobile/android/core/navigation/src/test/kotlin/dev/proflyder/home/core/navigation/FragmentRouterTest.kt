package dev.proflyder.home.core.navigation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Exercises the real support FragmentManager, its saved state, and the real
 * Activity lifecycle. No emulator or instrumentation is used.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FragmentRouterTest {
    @Test
    fun freshLaunchShowsOneTabRoot() {
        val activity = launch()

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertEquals(ALARMS_TITLE, activity.router.currentTitleRes.value)
        assertFalse(activity.router.canGoBack.value)
        assertFalse("at root, Back must let the Activity finish", activity.router.back())
    }

    @Test
    fun pushShowsEditorAndBackReturnsToRoot() {
        val activity = launch()

        activity.router.navigate(AlarmEditorDestination("7"))

        assertFragment(activity, "alarms/editor/7")
        assertEquals(1, activity.supportFragmentManager.backStackEntryCount)
        assertEquals(EDITOR_TITLE, activity.router.currentTitleRes.value)
        assertTrue(activity.router.canGoBack.value)

        assertTrue(activity.router.back())

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertEquals(ALARMS_TITLE, activity.router.currentTitleRes.value)
        assertFalse(activity.router.canGoBack.value)
    }

    @Test
    fun doublePushCreatesOneEditorAndOneBackToRoot() {
        val activity = launch()
        val editor = AlarmEditorDestination("7")

        activity.router.navigate(editor)
        activity.router.navigate(editor)

        assertFragment(activity, editor.route)
        assertEquals(1, activity.supportFragmentManager.backStackEntryCount)
        assertEquals(1, activity.supportFragmentManager.fragments.count { it.tag == editor.route })

        assertTrue(activity.router.back())
        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertFalse(activity.router.back())
    }

    @Test
    fun recreationRestoresSelectedEditorBackStackAndTitle() {
        val first = launch()
        first.router.navigate(AlarmEditorDestination("7"))

        val bundle = Bundle()
        first.controller.saveInstanceState(bundle).pause().stop().destroy()

        val second = launch(bundle)

        assertFragment(second, "alarms/editor/7")
        assertEquals(1, second.supportFragmentManager.backStackEntryCount)
        assertEquals("alarms", second.router.currentTabId.value)
        assertEquals(EDITOR_TITLE, second.router.currentTitleRes.value)
        assertTrue(second.router.canGoBack.value)

        assertTrue(second.router.back())
        assertFragment(second, "alarms/list")
        assertEquals(0, second.supportFragmentManager.backStackEntryCount)
        assertEquals(ALARMS_TITLE, second.router.currentTitleRes.value)
    }

    @Test
    fun savedBundleWithoutRestoredFragmentRecreatesSelectedRoot() {
        val malformed = Bundle().apply {
            putString(KEY_SELECTED_TAB, "timers")
            putStringArrayList(KEY_TABS, arrayListOf("timers"))
            putStringArrayList("${KEY_STACK_PREFIX}timers", arrayListOf("timers/list"))
        }

        val activity = launch(malformed)

        assertFragment(activity, "timers/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertEquals("timers", activity.router.currentTabId.value)
        assertEquals(TIMERS_TITLE, activity.router.currentTitleRes.value)
    }

    @Test
    fun malformedSavedBundleFallsBackSafelyInsteadOfLeavingBlankContainer() {
        val malformed = Bundle().apply {
            putString(KEY_SELECTED_TAB, "ghost")
            putStringArrayList(KEY_TABS, arrayListOf("ghost"))
            putStringArrayList("${KEY_STACK_PREFIX}ghost", arrayListOf("ghost/editor/1"))
        }

        val activity = launch(malformed)

        assertNotNull(currentFragment(activity))
        assertFragment(activity, "alarms/list")
        assertEquals("alarms", activity.router.currentTabId.value)
        assertEquals(ALARMS_TITLE, activity.router.currentTitleRes.value)
    }

    @Test
    fun savedEditorRouteWithoutFragmentFallsBackToRootNotBlank() {
        val saved = Bundle().apply {
            putString(KEY_SELECTED_TAB, "alarms")
            putStringArrayList(KEY_TABS, arrayListOf("alarms"))
            putStringArrayList(
                "${KEY_STACK_PREFIX}alarms",
                arrayListOf("alarms/list", "alarms/editor/7"),
            )
        }

        val activity = launch(saved)

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
    }

    @Test
    fun restoredFragmentWinsWhenRouterRouteStateIsMissing() {
        val first = launch()
        first.router.navigate(AlarmEditorDestination("7"))
        val bundle = Bundle()
        first.controller.saveInstanceState(bundle).pause().stop().destroy()
        // Simulate an old/corrupt bundle that lost the router metadata while
        // FragmentManager still restored the editor and its back stack.
        bundle.remove(KEY_SELECTED_TAB)
        bundle.remove(KEY_TABS)
        bundle.remove("${KEY_STACK_PREFIX}alarms")
        bundle.remove(KEY_TITLE_ROUTES)
        bundle.remove(KEY_TITLE_VALUES)

        val second = launch(bundle)

        assertFragment(second, "alarms/editor/7")
        assertEquals(1, second.supportFragmentManager.backStackEntryCount)
        assertEquals("alarms", second.router.currentTabId.value)
        assertTrue(second.router.canGoBack.value)

        assertTrue(second.router.back())
        assertFragment(second, "alarms/list")
    }

    @Test
    fun queuedNavigationWhilePausedIsNotCommittedUntilResume() {
        val activity = launch()
        activity.controller.pause()

        activity.router.navigate(AlarmEditorDestination("7"))

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)

        activity.controller.resume()

        assertFragment(activity, "alarms/editor/7")
        assertEquals(1, activity.supportFragmentManager.backStackEntryCount)
    }

    @Test
    fun queuedBackWhilePausedIsDrainedAfterResume() {
        val activity = launch()
        activity.router.navigate(AlarmEditorDestination("7"))
        activity.controller.pause()

        assertTrue("queued Back is accepted, not silently dropped", activity.router.back())
        assertFragment(activity, "alarms/editor/7")

        activity.controller.resume()

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertFalse(activity.router.back())
    }

    @Test
    fun backWhileStateSavedQueuesAndDrainsOnlyAfterStartResume() {
        val activity = launch()
        activity.router.navigate(AlarmEditorDestination("7"))

        val bundle = Bundle()
        activity.controller.saveInstanceState(bundle)
        assertTrue(activity.supportFragmentManager.isStateSaved)

        assertTrue(activity.router.back())
        assertFragment(activity, "alarms/editor/7")

        activity.controller.pause().stop().start().resume()

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
    }

    @Test
    fun backAtRootWhilePausedDoesNotSwallowFutureBack() {
        val activity = launch()
        activity.controller.pause()

        assertFalse(activity.router.back())

        activity.controller.resume()
        assertFragment(activity, "alarms/list")
        assertFalse(activity.router.back())
    }

    @Test
    fun tabSwitchResetsTargetTabToRoot() {
        val activity = launch()
        activity.router.navigate(AlarmEditorDestination("7"))
        assertFragment(activity, "alarms/editor/7")

        activity.router.selectTab("timers")

        assertFragment(activity, "timers/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertEquals("timers", activity.router.currentTabId.value)
        assertEquals(TIMERS_TITLE, activity.router.currentTitleRes.value)
        assertFalse(activity.router.canGoBack.value)

        activity.router.selectTab("alarms")

        assertFragment(activity, "alarms/list")
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
        assertEquals(ALARMS_TITLE, activity.router.currentTitleRes.value)
    }

    @Test
    fun sameRootSelectionDoesNotCreateAnotherTransaction() {
        val activity = launch()
        val root = currentFragment(activity)

        activity.router.selectTab("alarms")

        assertSame(root, currentFragment(activity))
        assertEquals(0, activity.supportFragmentManager.backStackEntryCount)
    }

    private fun launch(bundle: Bundle? = null): LaunchedActivity {
        val controller = if (bundle == null) {
            Robolectric.buildActivity(TestActivity::class.java).setup()
        } else {
            Robolectric.buildActivity(TestActivity::class.java)
                .create(bundle)
                .start()
                .resume()
                .visible()
        }
        return LaunchedActivity(controller, controller.get())
    }

    private fun assertFragment(activity: LaunchedActivity, tag: String) {
        assertEquals(tag, currentFragment(activity)?.tag)
    }

    private fun currentFragment(activity: LaunchedActivity): Fragment? =
        activity.supportFragmentManager.findFragmentById(TestActivity.CONTAINER_ID)

    private class LaunchedActivity(
        val controller: org.robolectric.android.controller.ActivityController<TestActivity>,
        val activity: TestActivity,
    ) {
        val router: FragmentRouter get() = activity.router
        val supportFragmentManager get() = activity.supportFragmentManager
    }

    private companion object {
        const val KEY_SELECTED_TAB = "router_selected_tab"
        const val KEY_TABS = "router_tabs"
        const val KEY_STACK_PREFIX = "router_stack_"
        const val KEY_TITLE_ROUTES = "router_title_routes"
        const val KEY_TITLE_VALUES = "router_title_values"
    }
}

class TestActivity : FragmentActivity() {
    lateinit var router: FragmentRouter
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val container = FrameLayout(this).apply { id = CONTAINER_ID }
        setContentView(container)

        router = FragmentRouter(
            fragmentManager = supportFragmentManager,
            containerId = CONTAINER_ID,
            registry = TEST_REGISTRY,
            savedState = FragmentRouter.RouterSavedState.from(savedInstanceState),
        )
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    if (!router.back()) finish()
                }
            },
        )
    }

    override fun onResume() {
        super.onResume()
        router.onHostResumed()
    }

    override fun onPause() {
        router.onHostPaused()
        super.onPause()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        router.onHostSaveState(outState)
        super.onSaveInstanceState(outState)
    }

    companion object {
        const val CONTAINER_ID = 0x00ff0001
    }
}

class TestFragment : Fragment() {
    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View = FrameLayout(requireContext())
}

object AlarmsRootDestination : Destination {
    override val featureId: String = "alarms"
    override val route: String = "alarms/list"
}

data class AlarmEditorDestination(val alarmId: String) : Destination {
    override val featureId: String = "alarms"
    override val route: String = "alarms/editor/$alarmId"

    override fun arguments(): Bundle = Bundle().apply { putString("alarm_id", alarmId) }
}

private object TimersRootDestination : Destination {
    override val featureId: String = "timers"
    override val route: String = "timers/list"
}

private class AlarmsEntry : FeatureEntry {
    override val id: String = "alarms"
    override val order: Int = 0
    override val titleRes: Int = ALARMS_TITLE
    override val iconRes: Int = 0
    override val rootDestination: Destination = AlarmsRootDestination

    override fun titleRes(destination: Destination): Int =
        if (destination is AlarmEditorDestination) EDITOR_TITLE else ALARMS_TITLE

    override fun supports(destination: Destination): Boolean =
        destination is AlarmsRootDestination || destination is AlarmEditorDestination

    override fun createFragment(destination: Destination): Fragment = TestFragment()
}

private class TimersEntry : FeatureEntry {
    override val id: String = "timers"
    override val order: Int = 1
    override val titleRes: Int = TIMERS_TITLE
    override val iconRes: Int = 0
    override val rootDestination: Destination = TimersRootDestination

    override fun supports(destination: Destination): Boolean =
        destination is TimersRootDestination

    override fun createFragment(destination: Destination): Fragment = TestFragment()
}

private val TEST_REGISTRY = FeatureRegistry(listOf(AlarmsEntry(), TimersEntry()))

private const val ALARMS_TITLE = 101
private const val EDITOR_TITLE = 102
private const val TIMERS_TITLE = 201
