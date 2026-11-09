package dev.proflyder.home

import androidx.viewpager2.widget.ViewPager2
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.tabs.TabLayout
import dev.proflyder.home.feature.alarms.presentation.AlarmsFragment
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
    fun bottomBarHasThreeContentTabsAndSettingsLiveInTheToolbar() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertEquals(3, bottom.menu.size())
        val titles = (0 until bottom.menu.size()).map { bottom.menu.getItem(it).title.toString() }
        assertEquals(listOf("Home", "Scenarios", "Time"), titles)

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        assertNotNull(toolbar.menu.findItem(R.id.action_settings))
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
        assertTrue(time.childFragmentManager.fragments.any { it is AlarmsFragment })
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
        val tabs = time.requireView().findViewById<TabLayout>(R.id.time_tabs)
        assertEquals(TimeSection.REMINDERS.ordinal, tabs.selectedTabPosition)
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
        val tabs = time.requireView().findViewById<TabLayout>(R.id.time_tabs)
        val pager = time.requireView().findViewById<ViewPager2>(R.id.time_pager)
        tabs.getTabAt(TimeSection.TIMERS.ordinal)?.select()
        shadowOf(activity.mainLooper).idle()

        assertEquals(TimeSection.TIMERS, time.section)
        assertEquals(TimeSection.TIMERS.ordinal, pager.currentItem)
    }
}
