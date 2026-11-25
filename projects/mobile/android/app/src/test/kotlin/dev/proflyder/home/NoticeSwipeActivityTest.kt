package dev.proflyder.home

import android.os.Looper
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import com.google.android.material.bottomnavigation.BottomNavigationView
import com.google.android.material.snackbar.Snackbar
import dev.proflyder.home.core.ui.NoticeKind
import dev.proflyder.home.core.ui.Notices
import java.time.Duration
import org.junit.After
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
class NoticeSwipeActivityTest {
    private var snackbar: Snackbar? = null

    @After
    fun closeNotice() {
        snackbar?.dismiss()
        snackbar = null
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(1_000))
    }

    private fun launch(): MainActivity {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        activity.window.decorView.measure(
            View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(HEIGHT, View.MeasureSpec.EXACTLY),
        )
        activity.window.decorView.layout(0, 0, WIDTH, HEIGHT)
        return activity
    }

    private fun showNotice(activity: MainActivity): View {
        val root = requireNotNull(
            activity.supportFragmentManager.findFragmentById(R.id.fragment_container)?.view,
        )
        val shown = Notices.show(root, "Проверьте локальную сеть или VPN", NoticeKind.Error, long = true)
        snackbar = shown
        shadowOf(activity.mainLooper).idle()
        assertNotNull(
            "the notice must be added to the activity window",
            activity.findViewById<TextView>(com.google.android.material.R.id.snackbar_text),
        )
        return shown.view
    }

    private fun layoutNotice(activity: MainActivity, notice: View) {
        val parent = notice.parent as ViewGroup
        parent.measure(
            View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(HEIGHT, View.MeasureSpec.EXACTLY),
        )
        parent.layout(0, 0, WIDTH, HEIGHT)
        activity.window.decorView.measure(
            View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(HEIGHT, View.MeasureSpec.EXACTLY),
        )
        activity.window.decorView.layout(0, 0, WIDTH, HEIGHT)
    }

    private fun View.swipeTo(x: Float, y: Float) {
        val startX = width / 2f
        val startY = height / 2f
        dispatchTouchEvent(MotionEvent.obtain(0L, 0L, MotionEvent.ACTION_DOWN, startX, startY, 0))
        dispatchTouchEvent(MotionEvent.obtain(0L, 0L, MotionEvent.ACTION_MOVE, x, y, 0))
        dispatchTouchEvent(MotionEvent.obtain(0L, 0L, MotionEvent.ACTION_UP, x, y, 0))
    }

    @Test
    fun noticeFloatsAboveTheBottomNavigation() {
        val activity = launch()
        val notice = showNotice(activity)
        layoutNotice(activity, notice)

        val bottom = activity.findViewById<BottomNavigationView>(R.id.home_notice_anchor)
        assertTrue("the bottom bar must be laid out", bottom.height > 0)
        val params = notice.layoutParams as ViewGroup.MarginLayoutParams
        assertTrue(
            "the notice needs a bottom margin equal to the navigation height, got ${params.bottomMargin}",
            params.bottomMargin >= bottom.height,
        )
        assertTrue(
            "the notice must sit above the bottom bar: ${notice.windowBottom()} > ${bottom.windowTop()}",
            notice.windowBottom() <= bottom.windowTop(),
        )
    }

    @Test
    fun sidewaysSwipeDismissesTheNotice() {
        val activity = launch()
        val notice = showNotice(activity)
        layoutNotice(activity, notice)

        notice.swipeTo(notice.width.toFloat() + 200f, notice.height / 2f)
        shadowOf(activity.mainLooper).idleFor(Duration.ofMillis(1_000))

        assertTrue(
            "the swiped notice must leave the window",
            activity.findViewById<View>(com.google.android.material.R.id.snackbar_text) == null,
        )
    }

    @Test
    fun downwardSwipeDismissesTheNotice() {
        val activity = launch()
        val notice = showNotice(activity)
        layoutNotice(activity, notice)

        notice.swipeTo(notice.width / 2f, notice.height.toFloat() + 400f)
        shadowOf(activity.mainLooper).idleFor(Duration.ofMillis(1_000))

        assertTrue(
            "the swiped notice must leave the window",
            activity.findViewById<View>(com.google.android.material.R.id.snackbar_text) == null,
        )
    }

    @Test
    fun tapLeavesTheNoticeVisible() {
        val activity = launch()
        val notice = showNotice(activity)
        layoutNotice(activity, notice)

        notice.swipeTo(notice.width / 2f, notice.height / 2f)
        shadowOf(activity.mainLooper).idleFor(Duration.ofMillis(500))

        assertNotNull(
            "a tap must not dismiss the notice",
            activity.findViewById<View>(com.google.android.material.R.id.snackbar_text),
        )
    }

    private companion object {
        const val WIDTH = 1080
        const val HEIGHT = 1920
    }
}

private fun View.windowTop(): Int = IntArray(2).also(::getLocationInWindow)[1]

private fun View.windowBottom(): Int = windowTop() + height
