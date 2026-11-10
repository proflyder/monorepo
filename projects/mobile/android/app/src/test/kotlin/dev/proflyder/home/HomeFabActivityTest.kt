package dev.proflyder.home

import android.view.View
import android.widget.LinearLayout
import androidx.fragment.app.Fragment
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.floatingactionbutton.FloatingActionButton
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.SpeedDialItems
import java.time.Duration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowWindowManagerImpl

/**
 * The root FAB lives in the Home fragment, but its speed dial is a window-level
 * popup. These tests drive the real MainActivity so a lost click listener or an
 * invisible overlay cannot pass unnoticed.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class HomeFabActivityTest {
    private fun launch(): MainActivity {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        return activity
    }

    private fun home(activity: MainActivity): Fragment =
        requireNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container))

    private fun fab(activity: MainActivity): FloatingActionButton =
        requireNotNull(home(activity).view).findViewById(R.id.control_quick_fab)

    /** Give the dial one item without touching the network. */
    private fun seedCallItem(activity: MainActivity) {
        val overlay = home(activity).javaClass
            .getDeclaredField("speedDial")
            .apply { isAccessible = true }
            .get(home(activity))
        val items = SpeedDialItems.build(listOf(QuickTile.Call), emptyList())
        overlay.javaClass
            .getMethod("render", List::class.java)
            .invoke(overlay, items)
    }

    private fun popupRoot(activity: MainActivity): View? {
        val windows = (shadowOf(activity.windowManager) as ShadowWindowManagerImpl).views
        return windows.firstNotNullOfOrNull { window ->
            window.findViewById<View>(R.id.control_dial_root)
        }
    }

    @Test
    fun fabOpensTheDialAndScrimAndBackCloseIt() {
        val activity = launch()
        seedCallItem(activity)
        assertNull(popupRoot(activity))

        fab(activity).performClick()
        shadowOf(activity.mainLooper).idle()
        val root = requireNotNull(popupRoot(activity))
        assertTrue(root.findViewById<View>(R.id.control_dial_scrim).isShown)

        val items = root.findViewById<LinearLayout>(R.id.control_dial_items)
        assertEquals(1, items.childCount)
        val item = items.getChildAt(0).findViewById<FloatingActionButton>(R.id.control_dial_action)
        assertTrue(item.isEnabled)
        assertTrue(item.hasOnClickListeners())

        root.findViewById<View>(R.id.control_dial_scrim).performClick()
        shadowOf(activity.mainLooper).idleFor(Duration.ofMillis(800))
        assertNull(popupRoot(activity))

        fab(activity).performClick()
        shadowOf(activity.mainLooper).idle()
        assertNotNull(popupRoot(activity))
        activity.onBackPressedDispatcher.onBackPressed()
        shadowOf(activity.mainLooper).idleFor(Duration.ofMillis(800))
        assertNull(popupRoot(activity))
    }

    @Test
    fun aClosedDialDoesNotInterceptTheContentOrBottomNavigation() {
        val activity = launch()
        assertNull(popupRoot(activity))

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        val scenarios = bottom.menu.getItem(1).itemId
        bottom.menu.performIdentifierAction(scenarios, 0)
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        assertEquals("scenarios", activity.router.currentTabId.value)
        assertNull(popupRoot(activity))
    }
}
