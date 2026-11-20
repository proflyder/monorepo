package dev.proflyder.home.core.ui

import android.os.Looper
import android.view.MotionEvent
import android.view.View
import java.time.Duration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NoticeSwipeDismissTest {
    private var dismissed = false

    private fun notice(): View {
        val view = View(RuntimeEnvironment.getApplication())
        view.measure(
            View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(HEIGHT, View.MeasureSpec.EXACTLY),
        )
        view.layout(0, 0, WIDTH, HEIGHT)
        return view
    }

    private fun View.swipe(
        vararg points: Pair<Float, Float>,
    ) {
        points.forEachIndexed { index, point ->
            val action = when (index) {
                0 -> MotionEvent.ACTION_DOWN
                points.lastIndex -> MotionEvent.ACTION_UP
                else -> MotionEvent.ACTION_MOVE
            }
            dispatchTouchEvent(MotionEvent.obtain(0L, 0L, action, point.first, point.second, 0))
        }
    }

    private fun runAnimations() {
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(500))
    }

    @Test
    fun sidewaysSwipeFollowsTheFingerAndDismisses() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(50f to 50f, 250f to 60f, 250f to 60f)
        assertTrue("the notice must move with the finger", view.translationX > 0f)
        assertTrue("the notice must fade while dragged", view.alpha < 1f)

        runAnimations()

        assertTrue("a sideways swipe must dismiss the notice", dismissed)
    }

    @Test
    fun downwardSwipeDismisses() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(50f to 50f, 70f to 110f, 70f to 110f)
        assertTrue("the notice must move down with the finger", view.translationY > 0f)

        runAnimations()

        assertTrue("a downward swipe must dismiss the notice", dismissed)
    }

    @Test
    fun leftSwipeDismisses() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(500f to 50f, 100f to 60f, 100f to 60f)
        assertTrue("the notice must move left with the finger", view.translationX < 0f)

        runAnimations()

        assertTrue("a left swipe must dismiss the notice", dismissed)
    }

    @Test
    fun tapDoesNotDismiss() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(50f to 50f, 50f to 50f)

        assertFalse("a tap must not dismiss the notice", dismissed)
        assertEquals(0f, view.translationX, 0f)
        assertEquals(0f, view.translationY, 0f)
    }

    @Test
    fun smallDragSnapsBack() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(50f to 50f, 100f to 50f, 100f to 50f)

        assertFalse("a short drag must not dismiss the notice", dismissed)
    }

    @Test
    fun upwardSwipeSnapsBack() {
        val view = notice()
        NoticeSwipeDismiss(view) { dismissed = true }.attach()

        view.swipe(50f to 150f, 50f to 40f, 50f to 40f)

        assertFalse("an upward swipe must not dismiss the notice", dismissed)
        assertEquals("the notice must not follow an upward drag", 0f, view.translationY, 0f)
    }

    private companion object {
        const val WIDTH = 600
        const val HEIGHT = 200
    }
}
