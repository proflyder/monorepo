package dev.proflyder.home.feature.settings.presentation

import android.os.Looper
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.feature.settings.impl.R
import java.time.Duration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import com.google.android.material.color.MaterialColors
import com.google.android.material.R as MaterialR
import org.robolectric.annotation.GraphicsMode
import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NotificationHistorySwipeFragmentTest {
    private val now = System.currentTimeMillis()
    private lateinit var repository: FakeHistoryRepository

    @Before
    fun setUp() {
        repository = FakeHistoryRepository(entries())
        val app = RuntimeEnvironment.getApplication() as HistoryTestApplication
        app.viewModel = NotificationHistoryViewModel(repository, testTargets(), FakeAccessSource(), FakeClock(now), CoroutineScope(Dispatchers.Main))
    }

    @Test
    fun swipingLeftDeletesTheRowAtRest() {
        val list = show()
        val key = firstItemKey(list)

        val row = rowOf(list, key)
        val revealed = swipe(list, row, -1)

        assertEquals(color(row, MaterialR.attr.colorError), revealed)
        assertFalse(key in keys())
    }

    @Test
    @Config(qualifiers = "w411dp-h891dp-night-xxhdpi")
    fun swipeActionsShowInDark() {
        val list = show()
        val key = firstItemKey(list)
        val row = rowOf(list, key)
        assertEquals(color(row, MaterialR.attr.colorError), swipe(list, row, -1))
        assertFalse(key in keys())
        val next = rowOf(list, null)
        assertEquals(color(next, androidx.appcompat.R.attr.colorPrimary), swipe(list, next, 1))
    }

    @Test
    fun swipingLeftDeletesARowUnderAPinnedHeader() {
        val list = show()
        val manager = list.layoutManager as StickyHeadersLayoutManager
        list.scrollBy(0, list.height / 2)
        settle(list)
        val pinned = assertNotNullAnd(manager.stickyHeaderView) { "a day header is pinned" }
        val row = (0 until list.childCount).map(list::getChildAt)
            .filter { it !== pinned && it.top >= pinned.bottom && it.bottom < list.height }
            .first { rowAt(list, it) is HistoryRow.Item }
        val key = keyOf(list, row)

        val revealed = swipe(list, row, -1)

        assertEquals(color(row, MaterialR.attr.colorError), revealed)
        assertFalse(key in keys())
    }

    @Test
    fun swipingRightTogglesReadUnderAPinnedHeader() {
        val list = show()
        list.scrollBy(0, list.height / 2)
        settle(list)
        val pinned = checkNotNull((list.layoutManager as StickyHeadersLayoutManager).stickyHeaderView)
        val row = (0 until list.childCount).map(list::getChildAt)
            .filter { it !== pinned && it.top >= pinned.bottom && it.bottom < list.height }
            .first { rowAt(list, it) is HistoryRow.Item }
        val key = keyOf(list, row)
        val before = repository.paged.state.value.items.first { it.key == key }.read

        val revealed = swipe(list, row, 1)

        assertEquals(color(row, androidx.appcompat.R.attr.colorPrimary), revealed)
        assertEquals(!before, repository.paged.state.value.items.first { it.key == key }.read)
    }

    @Test
    fun headersDoNotSwipe() {
        val list = show()
        val manager = list.layoutManager as StickyHeadersLayoutManager
        list.scrollBy(0, list.height / 2)
        settle(list)
        val pinned = checkNotNull(manager.stickyHeaderView)
        val before = keys()

        swipe(list, pinned, -1)
        val header = (0 until list.childCount).map(list::getChildAt)
            .first { it !== pinned && rowAt(list, it) is HistoryRow.Day && it.top > pinned.bottom }
        swipe(list, header, -1)

        assertEquals(before, keys())
    }

    private fun <T : Any> assertNotNullAnd(value: T?, message: () -> String): T {
        assertNotNull(message(), value)
        return checkNotNull(value)
    }

    private fun show(): RecyclerView {
        val activity = Robolectric.buildActivity(HistoryTestActivity::class.java).setup().get()
        activity.supportFragmentManager.beginTransaction().add(HistoryTestActivity.CONTAINER_ID, NotificationHistoryFragment()).commitNow()
        val list = activity.findViewById<RecyclerView>(R.id.history_list)
        settle(list)
        ViewCompat.dispatchApplyWindowInsets(list.rootView, chromeInsets())
        settle(list)
        assertTrue(list.childCount > 0)
        return list
    }

    private fun keys(): List<String> = repository.paged.state.value.items.map { it.key }

    private fun firstItemKey(list: RecyclerView): String = keyOf(list, rowOf(list, null))

    private fun rowOf(list: RecyclerView, key: String?): View = (0 until list.childCount).map(list::getChildAt)
        .first { rowAt(list, it) is HistoryRow.Item && (key == null || keyOf(list, it) == key) }

    private fun rowAt(list: RecyclerView, view: View): HistoryRow? {
        val holder = list.getChildViewHolder(view)
        val adapter = holder.bindingAdapter as? NotificationHistoryAdapter ?: return null
        return adapter.currentList.getOrNull(holder.bindingAdapterPosition)
    }

    private fun keyOf(list: RecyclerView, row: View): String = (rowAt(list, row) as HistoryRow.Item).entry.key

    private fun swipe(list: RecyclerView, row: View, sign: Int): Int {
        val y = row.top + row.height / 2f
        val startX = list.width / 2f
        val start = SystemClock.uptimeMillis()
        list.dispatchTouchEvent(MotionEvent.obtain(start, start, MotionEvent.ACTION_DOWN, startX, y, 0))
        (1..STEPS).forEach { step -> move(list, start, startX + sign * step * list.width / (2f * STEPS), y, step) }
        val revealed = revealedColor(list, row, sign)
        val endX = startX + sign * list.width / 2f
        list.dispatchTouchEvent(MotionEvent.obtain(start, start + (STEPS + 1) * STEP_MS, MotionEvent.ACTION_UP, endX, y, 0))
        settle(list)
        return revealed
    }

    private fun move(list: RecyclerView, start: Long, x: Float, y: Float, step: Int) {
        list.dispatchTouchEvent(MotionEvent.obtain(start, start + step * STEP_MS, MotionEvent.ACTION_MOVE, x, y, 0))
        frame(list)
    }

    private fun settle(list: RecyclerView) = repeat(PASSES) { settleOnce(list) }

    private fun settleOnce(list: RecyclerView) {
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(FRAME_MS))
        list.rootView.requestLayout()
        shadowOf(Looper.getMainLooper()).idle()
        frame(list)
    }

    private fun frame(list: RecyclerView) {
        val root = list.rootView
        val node = android.graphics.RenderNode("frame")
        node.setPosition(0, 0, root.width, root.height)
        val canvas = node.beginRecording(root.width, root.height)
        root.draw(canvas)
        node.endRecording()
    }

    private fun revealedColor(list: RecyclerView, row: View, sign: Int): Int {
        val root = list.rootView
        val bitmap = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))
        val at = IntArray(2).also(row::getLocationInWindow)
        val left = at[0] - row.translationX.toInt()
        val x = if (sign < 0) left + row.width - PROBE_INSET else left + PROBE_INSET
        val corner = if (sign < 0) left + row.width - CORNER_PROBE else left + CORNER_PROBE
        val center = bitmap.getPixel(x, at[1] + row.height / 2)
        val square = row.translationX == 0f || bitmap.getPixel(corner, at[1] + CORNER_DEPTH) == center
        assertTrue("action background is a full-width rectangle", square)
        return center
    }

    private fun color(view: View, attr: Int): Int = MaterialColors.getColor(view, attr)

    private fun chromeInsets(): WindowInsetsCompat = WindowInsetsCompat.Builder()
        .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, BAR, 0, 0))
        .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, NAV))
        .build()

    private fun entries(): List<NotificationHistory> = (0 until COUNT).map { index ->
        NotificationHistory(
            id = "e$index",
            type = NotificationHistoryType.NOTIFY,
            title = "Title $index",
            body = "Body $index",
            deepLink = "home://alarms",
            createdUnix = (now - index * SIX_HOURS) / MILLIS_PER_SECOND,
            read = false,
        )
    }

    private companion object {
        const val COUNT = 24
        const val SIX_HOURS = 6 * 3_600_000L
        const val MILLIS_PER_SECOND = 1_000L
        const val STEPS = 10
        const val STEP_MS = 16L
        const val FRAME_MS = 400L
        const val PASSES = 5
        const val BAR = 300
        const val PROBE_INSET = 10
        const val CORNER_PROBE = 4
        const val CORNER_DEPTH = 16
        const val NAV = 240
    }
}
