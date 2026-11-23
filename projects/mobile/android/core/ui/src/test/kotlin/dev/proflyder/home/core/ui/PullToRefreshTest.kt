package dev.proflyder.home.core.ui

import android.app.Activity
import android.view.ContextThemeWrapper
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.coordinatorlayout.widget.CoordinatorLayout
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PullToRefreshTest {
    private val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
    private val context = ContextThemeWrapper(activity, com.google.android.material.R.style.Theme_Material3_DayNight)
    private val list = RecyclerView(context).apply {
        layoutManager = LinearLayoutManager(context)
        adapter = Rows()
    }
    private val refresh = SwipeRefreshLayout(context).apply {
        addView(FrameLayout(context).apply { addView(CoordinatorLayout(context).apply { addView(list) }) })
    }

    @Test
    fun listAtTopLetsPullRefresh() {
        attach()
        assertFalse(refresh.canChildScrollUp())
    }

    @Test
    fun scrolledListBlocksPullRefreshThroughWrappers() {
        attach()
        list.scrollBy(0, SCROLL)
        assertTrue(refresh.canChildScrollUp())
    }

    @Test
    fun anEmptyFirstRowStillCountsAsTheTop() {
        list.adapter = Rows(firstHeight = 0)
        attach()
        assertFalse(refresh.canChildScrollUp())
        list.scrollBy(0, SCROLL)
        assertTrue(refresh.canChildScrollUp())
    }

    @Test
    fun collapsedHeaderBlocksPullRefresh() {
        val header = View(context)
        refresh.setupPullToRefresh(list, header) { }
        layout()
        header.offsetTopAndBottom(-SCROLL)
        assertTrue(refresh.canChildScrollUp())
    }

    private fun attach() {
        refresh.setupPullToRefresh(list) { }
        layout()
    }

    private fun layout() {
        activity.setContentView(refresh)
        refresh.measure(exactly(WIDTH), exactly(HEIGHT))
        refresh.layout(0, 0, WIDTH, HEIGHT)
    }

    private fun exactly(size: Int) = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private class Rows(private val firstHeight: Int = ROW_HEIGHT) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun getItemCount(): Int = ROWS

        override fun getItemViewType(position: Int): Int = if (position == 0) FIRST else OTHER

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder =
            object : RecyclerView.ViewHolder(row(parent, if (viewType == FIRST) firstHeight else ROW_HEIGHT)) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) = Unit

        private fun row(parent: ViewGroup, height: Int) = TextView(parent.context).apply {
            layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, height)
        }
    }

    private companion object {
        const val WIDTH = 400
        const val HEIGHT = 800
        const val ROWS = 50
        const val ROW_HEIGHT = 100
        const val SCROLL = 300
        const val FIRST = 0
        const val OTHER = 1
    }
}
