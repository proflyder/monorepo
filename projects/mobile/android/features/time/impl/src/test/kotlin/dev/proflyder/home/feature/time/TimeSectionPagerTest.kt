package dev.proflyder.home.feature.time

import android.os.Looper
import android.view.ViewGroup
import android.widget.TextView
import androidx.fragment.app.Fragment
import androidx.recyclerview.widget.RecyclerView
import androidx.viewpager2.widget.ViewPager2
import dev.proflyder.home.core.navigation.RouteArgs
import dev.proflyder.home.feature.time.impl.R
import dev.proflyder.home.feature.time.presentation.TimeFragment
import dev.proflyder.home.feature.time.presentation.showPage
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = TimeTestApplication::class)
class TimeSectionPagerTest {
    private val controller: ActivityController<TimeTestActivity> =
        Robolectric.buildActivity(TimeTestActivity::class.java).setup()

    @Test
    fun aHostedTimersRouteShowsTheTimersPage() {
        show(TimeSection.TIMERS)
        assertShown(TimeSection.TIMERS)
    }

    @Test
    fun replacingTheAlarmsScreenWithTheTimersRouteShowsTimers() {
        show(TimeSection.ALARMS)
        assertShown(TimeSection.ALARMS)
        show(TimeSection.TIMERS)
        assertShown(TimeSection.TIMERS)
    }

    @Test
    fun theTimersPageSurvivesRecreation() {
        show(TimeSection.TIMERS)
        controller.recreate()
        idle()
        assertShown(TimeSection.TIMERS)
    }

    @Test
    fun anAnimatedSelectionBeforeTheFirstLayoutStillLandsOnThePage() {
        val pager = ViewPager2(controller.get()).apply { adapter = Pages() }
        controller.get().setContentView(pager)
        pager.showPage(TimeSection.TIMERS.ordinal, animate = true)
        idle()
        assertEquals(TimeSection.TIMERS.ordinal, pager.currentItem)
        assertEquals(TimeSection.TIMERS.featureId, visiblePage(pager))
    }

    private fun show(section: TimeSection) {
        val fragment = TimeFragment().apply { arguments = RouteArgs.bundle(TimeDestination(section)) }
        controller.get().supportFragmentManager.beginTransaction()
            .setReorderingAllowed(true)
            .replace(android.R.id.content, fragment, TimeDestination(section).route)
            .commitAllowingStateLoss()
        controller.get().supportFragmentManager.executePendingTransactions()
        idle()
    }

    private fun assertShown(section: TimeSection) {
        val pager = pager()
        assertEquals(section.ordinal, pager.currentItem)
        assertEquals(section.featureId, visiblePage(pager))
    }

    private fun pager(): ViewPager2 {
        val host = controller.get().supportFragmentManager.fragments.single() as Fragment
        return host.requireView().findViewById(R.id.time_pager)
    }

    private fun visiblePage(pager: ViewPager2): CharSequence {
        val list = pager.getChildAt(0) as RecyclerView
        val visible = (0 until list.childCount).map(list::getChildAt).single { it.left == 0 && it.width > 0 }
        return ((visible as ViewGroup).getChildAt(0) as TextView).text
    }

    private fun idle() {
        repeat(IDLE_PASSES) {
            shadowOf(Looper.getMainLooper()).idle()
            controller.get().window.decorView.let { it.measure(EXACT_WIDTH, EXACT_HEIGHT); it.layout(0, 0, WIDTH, HEIGHT) }
        }
    }

    private class Pages : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val page = android.widget.FrameLayout(parent.context).apply {
                layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                addView(TextView(parent.context))
            }
            return object : RecyclerView.ViewHolder(page) {}
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            ((holder.itemView as ViewGroup).getChildAt(0) as TextView).text = TimeSection.entries[position].featureId
        }

        override fun getItemCount(): Int = TimeSection.entries.size
    }

    private companion object {
        const val WIDTH = 1080
        const val HEIGHT = 2000
        const val IDLE_PASSES = 3
        val EXACT_WIDTH = android.view.View.MeasureSpec.makeMeasureSpec(WIDTH, android.view.View.MeasureSpec.EXACTLY)
        val EXACT_HEIGHT = android.view.View.MeasureSpec.makeMeasureSpec(HEIGHT, android.view.View.MeasureSpec.EXACTLY)
    }
}
