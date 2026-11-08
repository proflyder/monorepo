package dev.proflyder.home

import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.bottomnavigation.BottomNavigationView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
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
    fun bottomBarHasFiveContentTabsAndSettingsLiveInTheToolbar() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertEquals(5, bottom.menu.size())
        val titles = (0 until bottom.menu.size()).map { bottom.menu.getItem(it).title.toString() }
        assertEquals(listOf("Home", "Scenarios", "Alarms", "Timers", "Reminders"), titles)

        val toolbar = activity.findViewById<MaterialToolbar>(R.id.toolbar)
        assertNotNull(toolbar.menu.findItem(R.id.action_settings))
    }
}
