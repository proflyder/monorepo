package dev.proflyder.home.core.designsystem.search

import android.app.Activity
import android.os.Looper
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w360dp-h640dp-xxhdpi")
class PullRevealSearchTest {
    private lateinit var search: PullRevealSearch
    private lateinit var field: EditText
    private lateinit var list: RecyclerView
    private val reveals = mutableListOf<Boolean>()

    @Before
    fun setUp() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        field = EditText(activity)
        search = PullRevealSearch(activity).apply { addView(field, FrameLayout.LayoutParams(MATCH, FIELD_HEIGHT)) }
        list = RecyclerView(activity).apply {
            layoutManager = LinearLayoutManager(activity)
            adapter = Rows()
        }
        val root = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL }
        root.addView(search, LinearLayout.LayoutParams(MATCH, ViewGroup.LayoutParams.WRAP_CONTENT))
        root.addView(list, LinearLayout.LayoutParams(MATCH, 0, 1f))
        activity.setContentView(root)
        search.attach(list, field)
        search.onRevealChanged = { reveals += it }
        settle()
    }

    @Test
    fun aPullDownRevealsAndASwipeBackUpHidesAnEmptySearch() {
        drag(from = 100f, to = 400f)
        assertTrue(search.isRevealed)
        assertEquals(search.fullHeight, search.offset)

        drag(from = 400f, to = 100f)
        assertFalse(search.isRevealed)
        assertEquals(0, search.offset)
        assertEquals(listOf(true, false), reveals)
    }

    @Test
    fun aSearchWithTextStaysWhenSwipedUp() {
        drag(from = 100f, to = 400f)
        field.text.append(QUERY)
        settle()
        drag(from = 400f, to = 100f)
        assertTrue(search.isRevealed)
    }

    @Test
    fun touchingTheListDropsFocusAndHidesAnEmptySearch() {
        drag(from = 100f, to = 400f)
        field.requestFocus()
        assertTrue(field.hasFocus())
        tap()
        assertFalse(field.hasFocus())
        assertEquals(0, search.offset)
    }

    @Test
    fun dismissClearsTheQueryAndHides() {
        drag(from = 100f, to = 400f)
        field.text.append(QUERY)
        field.requestFocus()
        search.dismiss()
        settle()
        assertEquals("", field.text.toString())
        assertFalse(field.hasFocus())
        assertEquals(0, search.offset)
        assertEquals(false, reveals.last())
    }

    private fun drag(from: Float, to: Float) {
        val start = SystemClock.uptimeMillis()
        list.dispatchTouchEvent(event(start, MotionEvent.ACTION_DOWN, from))
        STEPS.forEach { step -> list.dispatchTouchEvent(event(start + step, MotionEvent.ACTION_MOVE, from + (to - from) * step / STEPS.last())) }
        list.dispatchTouchEvent(event(start + STEPS.last() + 1, MotionEvent.ACTION_UP, to))
        settle()
    }

    private fun tap() {
        val start = SystemClock.uptimeMillis()
        list.dispatchTouchEvent(event(start, MotionEvent.ACTION_DOWN, 200f))
        list.dispatchTouchEvent(event(start + 1, MotionEvent.ACTION_UP, 200f))
        settle()
    }

    private fun event(time: Long, action: Int, y: Float): MotionEvent = MotionEvent.obtain(time, time, action, 100f, y, 0)

    private fun settle() {
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1))
        search.measure(
            View.MeasureSpec.makeMeasureSpec(search.width.coerceAtLeast(1), View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        shadowOf(Looper.getMainLooper()).idle()
    }

    private class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder =
            object : RecyclerView.ViewHolder(TextView(parent.context).apply { minHeight = ROW_HEIGHT }) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = position.toString()
        }

        override fun getItemCount(): Int = ROW_COUNT
    }

    private companion object {
        const val MATCH = ViewGroup.LayoutParams.MATCH_PARENT
        const val FIELD_HEIGHT = 150
        const val ROW_HEIGHT = 120
        const val ROW_COUNT = 3
        const val QUERY = "lamp"
        val STEPS = (1..10).map { it * 10L }
    }
}
