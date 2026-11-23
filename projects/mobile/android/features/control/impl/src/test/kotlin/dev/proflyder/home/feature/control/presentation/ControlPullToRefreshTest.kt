package dev.proflyder.home.feature.control.presentation

import android.app.Activity
import android.os.Looper
import android.os.SystemClock
import android.view.LayoutInflater
import android.view.MotionEvent
import android.view.View
import androidx.appcompat.view.ContextThemeWrapper
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.ControlCatalogState
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi")
class ControlPullToRefreshTest {
    private val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
    private val context = ContextThemeWrapper(activity, com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar)
    private val binding = FragmentControlBinding.inflate(LayoutInflater.from(context))
    private var refreshes = 0

    @Test
    fun aPullFromTheTopRefreshes() {
        show()
        assertFalse(binding.controlRefresh.canChildScrollUp())
        pull()
        assertTrue(binding.controlRefresh.isRefreshing)
    }

    @Test
    fun aPullInTheMiddleOfTheListDoesNotRefresh() {
        show()
        binding.controlList.scrollBy(0, MID_LIST)
        layout()
        assertTrue(binding.controlRefresh.canChildScrollUp())
        pull()
        assertFalse(binding.controlRefresh.isRefreshing)
        assertEquals(0, refreshes)
    }

    private fun show() {
        activity.setContentView(binding.root)
        ControlListRenderer(binding) { }.render(input())
        wirePullToRefresh(binding) { refreshes += 1 }
        layout()
        layout()
    }

    private fun layout() {
        val root = binding.root
        root.measure(exactly(WIDTH), exactly(HEIGHT))
        root.layout(0, 0, WIDTH, HEIGHT)
    }

    private fun pull() {
        val view = binding.controlRefresh
        val x = WIDTH / 2f
        val start = SystemClock.uptimeMillis()
        view.dispatchTouchEvent(event(start, MotionEvent.ACTION_DOWN, x, TOP))
        (1..STEPS).forEach { step -> view.dispatchTouchEvent(event(start + step * 16L, MotionEvent.ACTION_MOVE, x, TOP + step * STEP)) }
        view.dispatchTouchEvent(event(start + STEPS * 16L + 16L, MotionEvent.ACTION_UP, x, TOP + STEPS * STEP))
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun event(time: Long, action: Int, x: Float, y: Float): MotionEvent =
        MotionEvent.obtain(time - 100, time, action, x, y, 0)

    private fun input() = ControlListInput(
        catalog = ControlCatalogState(loading = false, catalog = ControlCatalog(entries(), statesOk = true)),
        hub = HubView(),
        filter = ControlFilter.All,
        pending = emptySet(),
        expandedDevices = emptySet(),
        collapsedRooms = emptySet(),
    )

    private fun entries(): List<ControlEntry> = (1..ENTRIES).map { index ->
        ControlEntry(
            id = "light.lamp$index",
            kind = ControlKind.SWITCH,
            title = "Лампа $index",
            room = "Комната ${index / ROOM_SIZE}",
            icon = "power",
            actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
            power = PowerState.ON,
            available = true,
        )
    }

    private fun exactly(size: Int) = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val WIDTH = 1233
        const val HEIGHT = 2400
        const val ENTRIES = 60
        const val ROOM_SIZE = 6
        const val MID_LIST = 2000
        const val TOP = 400f
        const val STEPS = 30
        const val STEP = 30f
    }
}
