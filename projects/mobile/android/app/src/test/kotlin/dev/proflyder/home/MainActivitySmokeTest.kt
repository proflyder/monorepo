package dev.proflyder.home

import android.view.View
import android.widget.ImageView
import android.widget.TextView
import androidx.viewpager2.widget.ViewPager2
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.bottomnavigation.BottomNavigationView
import dev.proflyder.home.core.designsystem.PillSegments
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import dev.proflyder.home.core.ui.R as UiR
import dev.proflyder.home.feature.alarms.impl.R as AlarmsR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class MainActivitySmokeTest {
    @Test
    fun coldLaunchCreatesActivityAndRootFragmentWithoutCrashing() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        assertFalse(activity.isFinishing)
        assertNotNull(activity.findViewById<android.view.View>(R.id.fragment_container))
        assertNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container))
    }

    @Test
    fun bottomBarHasFiveContentTabsAndOneToolbarAction() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertEquals(5, bottom.menu.size())
        val titles = (0 until bottom.menu.size()).map { bottom.menu.getItem(it).title.toString() }
        assertEquals(listOf("Home", "Scenarios", "Time", "Chat", "System"), titles)

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        assertEquals(1, toolbar.menu.size())
        val actions = toolbar.menu.getItem(0)
        assertEquals(R.id.action_menu, actions.itemId)
        assertEquals("Actions", actions.contentDescription)
        val actionView = toolbar.findViewById<View>(R.id.action_menu)
        assertNotNull("the actions button must have a visible view", actionView)
        assertEquals(View.VISIBLE, actionView.visibility)
    }

    @Test
    fun theActionsButtonOpensTheCustomMenuWithIconRows() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        toolbar.menu.performIdentifierAction(R.id.action_menu, 0)
        shadowOf(activity.mainLooper).idle()

        val opened = activity.actionsMenu.contentView
        assertNotNull("the actions button must open the custom menu", opened)
        val content = requireNotNull(opened)
        val call = content.findViewById<View>(R.id.menu_action_call)
        val settings = content.findViewById<View>(R.id.menu_action_settings)
        assertNotNull(call)
        assertNotNull(settings)
        assertEquals(View.VISIBLE, call.visibility)
        assertEquals(View.VISIBLE, settings.visibility)

        val callIcon = call.findViewById<ImageView>(UiR.id.ds_popup_menu_icon)
        val settingsIcon = settings.findViewById<ImageView>(UiR.id.ds_popup_menu_icon)
        assertNotNull("call row must carry a phone icon", callIcon.drawable)
        assertNotNull("settings row must carry a gear icon", settingsIcon.drawable)

        val callLabel = call.findViewById<TextView>(UiR.id.ds_popup_menu_label)
        val settingsLabel = settings.findViewById<TextView>(UiR.id.ds_popup_menu_label)
        assertEquals(activity.getString(R.string.app_call), callLabel.text.toString())
        assertEquals(activity.getString(R.string.app_settings), settingsLabel.text.toString())
    }

    @Test
    fun pickingSettingsOpensItAndLeavesOnlyBackAndTitle() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        toolbar.menu.performIdentifierAction(R.id.action_menu, 0)
        shadowOf(activity.mainLooper).idle()
        val opened = requireNotNull(activity.actionsMenu.contentView)
        opened.findViewById<View>(R.id.menu_action_settings).performClick()
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        assertNotNull(
            "the settings row must open the settings list",
            activity.supportFragmentManager.findFragmentByTag("settings/hub"),
        )
        assertFalse(
            "inside settings the actions button must hide",
            toolbar.menu.findItem(R.id.action_menu).isVisible,
        )
        assertEquals(
            "the profile chip belongs to the main tabs only",
            View.GONE,
            activity.findViewById<View>(R.id.profile_chip).visibility,
        )

        activity.router.back()
        shadowOf(activity.mainLooper).idle()
        assertTrue(
            "back to a tab must restore the actions button",
            toolbar.menu.findItem(R.id.action_menu).isVisible,
        )
    }

    @Test
    fun selectingTheAlarmsFeatureOpensTheTimeTabOnAlarms() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        activity.router.selectTab("alarms")
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        val time = activity.supportFragmentManager.findFragmentById(R.id.fragment_container)
        assertTrue("selecting alarms must open the Time container", time is TimeFragment)
        assertEquals(TimeSection.ALARMS, (time as TimeFragment).section)
        time.childFragmentManager.executePendingTransactions()
        assertTrue(time.childFragmentManager.fragments.any { it.view?.findViewById<View>(AlarmsR.id.alarms_list) != null })
        assertEquals(0, time.requireView().findViewById<ViewPager2>(R.id.time_pager).currentItem)

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertEquals("Time", bottom.menu.findItem(bottom.selectedItemId).title.toString())
    }

    @Test
    fun selectingTheTimersFeatureOpensTheTimeTabOnTimers() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        activity.router.selectTab("timers")
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        val time = activity.supportFragmentManager.findFragmentById(R.id.fragment_container) as TimeFragment
        assertEquals(TimeSection.TIMERS, time.section)
    }

    @Test
    fun swipingTheTimePagerSelectsTheMatchingPill() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        activity.router.selectTab("alarms")
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        val time = activity.supportFragmentManager.findFragmentById(R.id.fragment_container) as TimeFragment
        val pager = time.requireView().findViewById<ViewPager2>(R.id.time_pager)
        pager.setCurrentItem(TimeSection.REMINDERS.ordinal, false)
        shadowOf(activity.mainLooper).idle()

        assertEquals(TimeSection.REMINDERS, time.section)
        val tabs = time.requireView().findViewById<PillSegments>(R.id.time_tabs)
        assertEquals(TimeSection.REMINDERS.ordinal, tabs.selectedIndex)
    }

    @Test
    fun tappingATimePillMovesThePager() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        activity.router.selectTab("alarms")
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        val time = activity.supportFragmentManager.findFragmentById(R.id.fragment_container) as TimeFragment
        val tabs = time.requireView().findViewById<PillSegments>(R.id.time_tabs)
        val pager = time.requireView().findViewById<ViewPager2>(R.id.time_pager)
        tabs.getChildAt(TimeSection.TIMERS.ordinal).performClick()
        shadowOf(activity.mainLooper).idle()

        assertEquals(TimeSection.TIMERS, time.section)
        assertEquals(TimeSection.TIMERS.ordinal, pager.currentItem)
    }
}
