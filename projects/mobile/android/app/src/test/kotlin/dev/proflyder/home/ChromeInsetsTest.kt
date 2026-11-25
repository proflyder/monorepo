package dev.proflyder.home

import android.view.View
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.RecyclerView
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import dev.proflyder.home.core.ui.R as UiR
import dev.proflyder.home.feature.alarms.impl.R as AlarmsR
import dev.proflyder.home.feature.control.impl.R as ControlR
import dev.proflyder.home.feature.time.impl.R as TimeR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-night-xxhdpi")
class ChromeInsetsTest {
    @Test
    fun contentIsFullBleedUnderBothChromeBars() {
        val activity = open()
        val container = activity.findViewById<View>(R.id.fragment_container)
        val decor = activity.window.decorView
        assertEquals(0, container.windowTop())
        assertEquals(decor.height, container.height)
        assertEquals(0, activity.findViewById<View>(R.id.top_chrome).windowTop())
        assertEquals(decor.height, activity.findViewById<View>(R.id.bottom_chrome).windowBottom())
    }

    @Test
    fun chromeSizesArePublishedAsContentInsets() {
        val activity = open()
        val top = activity.findViewById<View>(R.id.top_chrome)
        val bottom = activity.findViewById<View>(R.id.bottom_chrome)
        assertEquals(dp(activity, STATUS_DP), top.paddingTop)
        assertEquals(dp(activity, NAV_DP), bottom.paddingBottom)
        val header = activity.findViewById<View>(ControlR.id.control_header)
        val list = activity.findViewById<RecyclerView>(ControlR.id.control_list)
        assertEquals("home list starts under the toolbar", top.height, list.paddingTop)
        assertEquals("alarm slot is the first list item", top.height.toFloat(), header.translationY)
        val fab = activity.resources.getDimensionPixelSize(UiR.dimen.ui_fab_clearance)
        assertTrue("home list clears the bottom chrome", list.paddingBottom >= bottom.height + fab)
    }

    @Test
    fun pullToRefreshSpinnerStartsUnderTheChromeOnHome() {
        val activity = open()
        val top = activity.findViewById<View>(R.id.top_chrome).height
        assertSpinnerBelow(activity.findViewById(ControlR.id.control_refresh), top)
    }

    @Test
    fun pullToRefreshSpinnerStartsUnderTheTimeTabs() {
        val activity = open()
        activity.router.selectTab("time")
        settle(activity)
        val tabs = activity.findViewById<View>(TimeR.id.time_header)
        assertSpinnerBelow(activity.findViewById(AlarmsR.id.alarms_refresh), tabs.height)
    }

    private fun assertSpinnerBelow(refresh: SwipeRefreshLayout, top: Int) {
        val travel = refresh.resources.getDimensionPixelSize(UiR.dimen.ui_refresh_travel)
        assertEquals("spinner hides right under the glass", top - refresh.progressCircleDiameter, refresh.progressViewStartOffset)
        assertEquals("spinner stops below the glass", top + travel, refresh.progressViewEndOffset)
    }

    private fun open(): MainActivity {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        settle(activity)
        activity.router.selectTab("control")
        settle(activity)
        activity.findViewById<View>(ControlR.id.control_content).visibility = View.VISIBLE
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, dp(activity, STATUS_DP), 0, 0))
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, dp(activity, NAV_DP)))
            .build()
        repeat(2) {
            ViewCompat.dispatchApplyWindowInsets(activity.window.decorView, insets)
            settle(activity)
        }
        return activity
    }

    private fun settle(activity: MainActivity) = repeat(PASSES) { settleOnce(activity) }

    private fun settleOnce(activity: MainActivity) {
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        activity.window.decorView.requestLayout()
        shadowOf(activity.mainLooper).idle()
    }

    private fun dp(activity: MainActivity, value: Int): Int = (value * activity.resources.displayMetrics.density).toInt()

    private companion object {
        const val STATUS_DP = 32
        const val NAV_DP = 24
        const val PASSES = 3
    }
}

private fun View.windowTop(): Int = IntArray(2).also(::getLocationInWindow)[1]

private fun View.windowBottom(): Int = windowTop() + height
