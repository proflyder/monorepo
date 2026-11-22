package dev.proflyder.home.feature.alarms.presentation

import android.os.Looper
import android.view.View
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import androidx.recyclerview.widget.SimpleItemAnimator
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.feature.alarms.impl.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w360dp-h800dp")
class AlarmAdapterPayloadTest {
    private val alarm = Schedule(
        id = "alarm-1",
        label = "Work",
        kind = ScheduleKind.ALARM,
        at = WallTime(10, 0),
        onceDate = "",
        repeat = RepeatKind.WEEKDAYS,
        weekdays = listOf(1, 2, 3, 4, 5),
        nextDueUnix = 0,
        enabled = true,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        activeOccurrenceUnix = 0,
        activeUntilUnix = 0,
    )

    @Test
    fun toggleReportsOnlyTheEnabledAspect() {
        val on = AlarmRow(alarm, selected = false, selectionMode = false)
        val off = on.copy(alarm = alarm.copy(enabled = false))
        assertEquals(PayloadSet.of(AlarmAspect.ENABLED), AlarmDiff.getChangePayload(on, off))
        assertEquals(PayloadSet.of(AlarmAspect.SELECTION), AlarmDiff.getChangePayload(on, on.copy(selected = true)))
        assertEquals(PayloadSet.of(AlarmAspect.CONTENT), AlarmDiff.getChangePayload(on, on.copy(alarm = alarm.copy(label = "Gym"))))
    }

    @Test
    fun toggleUpdatesTheSwitchInPlaceWithoutRebindingTheCard() {
        val list = recycler()
        submit(list, AlarmRow(alarm, selected = false, selectionMode = false))
        val holder = list.findViewHolderForAdapterPosition(0)!!
        val label = holder.itemView.findViewById<TextView>(R.id.alarm_label)
        label.text = MARKER

        submit(list, AlarmRow(alarm.copy(enabled = false), selected = false, selectionMode = false))

        assertFalse((list.itemAnimator as SimpleItemAnimator).supportsChangeAnimations)
        assertSame(holder, list.findViewHolderForAdapterPosition(0))
        assertEquals(MARKER, label.text.toString())
        assertFalse(holder.itemView.findViewById<MaterialSwitch>(R.id.alarm_enabled).isChecked)

        submit(list, AlarmRow(alarm.copy(enabled = false, label = "Gym"), selected = false, selectionMode = false))
        assertSame(holder, list.findViewHolderForAdapterPosition(0))
        assertEquals("Gym", label.text.toString())
    }

    private fun recycler(): RecyclerView {
        val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), com.google.android.material.R.style.Theme_Material3_DayNight)
        val list = RecyclerView(context)
        list.layoutManager = LinearLayoutManager(context)
        list.adapter = AlarmAdapter(AlarmAdapterCallback {})
        list.measure(exactly(1080), exactly(2400))
        list.layout(0, 0, 1080, 2400)
        return list
    }

    private fun submit(list: RecyclerView, row: AlarmRow) {
        var committed = false
        (list.adapter as AlarmAdapter).submitList(listOf(row)) { committed = true }
        repeat(LOOPS) { if (!committed) idle() }
        list.measure(exactly(1080), exactly(2400))
        list.layout(0, 0, 1080, 2400)
        idle()
    }

    private fun idle() {
        Thread.sleep(SLEEP_MILLIS)
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val MARKER = "marker"
        const val LOOPS = 100
        const val SLEEP_MILLIS = 10L
    }
}
