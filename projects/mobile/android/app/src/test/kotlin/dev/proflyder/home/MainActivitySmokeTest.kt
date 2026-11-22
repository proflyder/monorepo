package dev.proflyder.home

import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.Fragment
import android.widget.ImageView
import android.widget.TextView
import androidx.viewpager2.widget.ViewPager2
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.bottomnavigation.BottomNavigationView
import dev.proflyder.home.core.designsystem.PillSegments
import dev.proflyder.home.feature.time.TimeSection
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
import dev.proflyder.home.feature.call.impl.R as CallR
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.settings.impl.R as SettingsR
import dev.proflyder.home.feature.time.impl.R as TimeR
import dev.proflyder.home.feature.vpn.impl.R as VpnR

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
        val call = content.findViewById<View>(CallR.id.call_toolbar_action)
        val settings = content.findViewById<View>(SettingsR.id.settings_toolbar_action)
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
        assertEquals(activity.getString(CallR.string.call_toolbar_action), callLabel.text.toString())
        assertEquals(activity.getString(SettingsR.string.settings_toolbar_action), settingsLabel.text.toString())
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
        opened.findViewById<View>(SettingsR.id.settings_toolbar_action).performClick()
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
            activity.findViewById<View>(ControlR.id.control_profile_chip).visibility,
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
        val activity = launchedOn("alarms")

        val time = timeFragment(activity)
        assertEquals("time/alarms", time.tag)
        time.childFragmentManager.executePendingTransactions()
        assertTrue(time.childFragmentManager.fragments.any { it.view?.findViewById<View>(AlarmsR.id.alarms_list) != null })
        assertEquals(TimeSection.ALARMS.ordinal, pager(time).currentItem)

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertEquals("Time", bottom.menu.findItem(bottom.selectedItemId).title.toString())
    }

    @Test
    fun selectingTheTimersFeatureOpensTheTimeTabOnTimers() {
        val activity = launchedOn("timers")

        val time = timeFragment(activity)
        assertEquals("time/timers", time.tag)
        assertEquals(TimeSection.TIMERS.ordinal, pager(time).currentItem)
    }

    @Test
    fun swipingTheTimePagerSelectsTheMatchingPill() {
        val activity = launchedOn("alarms")
        val time = timeFragment(activity)

        pager(time).setCurrentItem(TimeSection.REMINDERS.ordinal, false)
        shadowOf(activity.mainLooper).idle()

        assertEquals(TimeSection.REMINDERS.ordinal, tabs(time).selectedIndex)
    }

    @Test
    fun tappingATimePillMovesThePager() {
        val activity = launchedOn("alarms")
        val time = timeFragment(activity)

        tabs(time).getChildAt(TimeSection.TIMERS.ordinal).performClick()
        shadowOf(activity.mainLooper).idle()

        assertEquals(TimeSection.TIMERS.ordinal, pager(time).currentItem)
        assertEquals(TimeSection.TIMERS.ordinal, tabs(time).selectedIndex)
    }

    @Test
    fun theProfileChipIsContributedByTheControlFeature() {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        shadowOf(activity.mainLooper).idle()

        val row = activity.findViewById<ViewGroup>(R.id.toolbar_title_row)
        assertNotNull(row.findViewById<View>(ControlR.id.control_profile_chip))
        assertNotNull(activity.findViewById<View>(R.id.toolbar_end_slot).findViewById<View>(VpnR.id.vpn_toolbar_toggle))
        assertNotNull(activity.findViewById<View>(R.id.appbar_below_slot).findViewById<View>(VpnR.id.vpn_route_banner))
    }

    private fun launchedOn(featureId: String): MainActivity {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        shadowOf(activity.mainLooper).idle()
        activity.router.selectTab(featureId)
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        return activity
    }

    private fun timeFragment(activity: MainActivity): Fragment =
        checkNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container))

    private fun pager(time: Fragment): ViewPager2 = time.requireView().findViewById(TimeR.id.time_pager)

    private fun tabs(time: Fragment): PillSegments = time.requireView().findViewById(TimeR.id.time_tabs)
}
