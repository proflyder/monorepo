package dev.proflyder.home.feature.control.presentation

import android.view.View
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.WakeFeedback
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * Robolectric smoke test of the "Voice activator" section: both marks must be
 * visible with localized labels, and a tap must report exactly that action.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ControlWakeFeedbackRowTest {
    private val marks = mutableListOf<WakeFeedbackKind>()

    private val callback = object : BaseAdapterCallback<ControlItem>(), ControlAdapterCallback {
        override fun onItemClick(item: ControlItem) = Unit

        override fun onPowerChanged(entry: ControlEntry, on: Boolean) = Unit

        override fun onProfileChanged(entry: ControlEntry, work: Boolean) = Unit

        override fun onToggleSettings(deviceId: String) = Unit

        override fun onHeaderClick(header: ControlItem.Header) = Unit

        override fun onPinToggle(entry: ControlEntry) = Unit

        override fun onHubCardClick(card: HubCard) = Unit

        override fun onHubPrimary(card: HubCard) = Unit

        override fun onQuickTileClick(tile: QuickTile) = Unit

        override fun onQuickTileLongClick(tile: QuickTile) = Unit

        override fun onWakeAction(action: WakeFeedbackAction) {
            marks += action.kind
        }

        override fun onFilterSelected(filter: ControlFilter) = Unit
    }

    private fun entry(id: String, available: Boolean = true) = ControlEntry(
        id = id,
        kind = ControlKind.SCENARIO,
        title = "Script title from server",
        room = "",
        icon = "",
        actions = setOf(ControlAction.RUN),
        power = null,
        available = available,
    )

    private fun action(kind: WakeFeedbackKind, available: Boolean = true, busy: Boolean = false) = WakeFeedbackAction(
        kind = kind,
        entry = entry(
            if (kind == WakeFeedbackKind.FALSE_ALARM) WakeFeedback.FALSE_ALARM_ENTRY_ID else WakeFeedback.MISSED_ENTRY_ID,
            available,
        ),
        busy = busy,
    )

    private class Row(val root: View, val falseAlarm: MaterialButton, val missed: MaterialButton)

    private fun bind(item: ControlItem.WakeFeedback): Row {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        val parent = FrameLayout(themed)
        val adapter = ControlAdapter(callback)
        adapter.submitList(listOf(item))
        val holder = adapter.onCreateViewHolder(parent, adapter.getItemViewType(0))
        adapter.onBindViewHolder(holder, 0)
        return Row(
            holder.itemView,
            holder.itemView.findViewById(R.id.control_wake_false_alarm),
            holder.itemView.findViewById(R.id.control_wake_missed),
        )
    }

    @Test
    fun bothMarksAreVisibleAndTapSendsThem() {
        val row = bind(ControlItem.WakeFeedback(listOf(action(WakeFeedbackKind.FALSE_ALARM), action(WakeFeedbackKind.MISSED))))
        assertEquals(View.VISIBLE, row.falseAlarm.visibility)
        assertEquals(View.VISIBLE, row.missed.visibility)
        assertEquals("False activation", row.falseAlarm.text.toString())
        assertEquals("Didn’t hear", row.missed.text.toString())

        row.falseAlarm.performClick()
        row.missed.performClick()
        assertEquals(listOf(WakeFeedbackKind.FALSE_ALARM, WakeFeedbackKind.MISSED), marks)
    }

    @Test
    fun unavailableOrBusyMarkCannotBeTapped() {
        val unavailable = bind(
            ControlItem.WakeFeedback(listOf(action(WakeFeedbackKind.MISSED, available = false))),
        )
        assertFalse(unavailable.missed.isEnabled)
        unavailable.missed.performClick()
        assertEquals(emptyList<WakeFeedbackKind>(), marks)

        val busy = bind(
            ControlItem.WakeFeedback(listOf(action(WakeFeedbackKind.MISSED, busy = true))),
        )
        assertFalse(busy.missed.isEnabled)
    }

    @Test
    fun aMissingScriptHidesItsButtonAndTheOtherStillWorks() {
        val row = bind(ControlItem.WakeFeedback(listOf(action(WakeFeedbackKind.MISSED))))
        assertEquals(View.GONE, row.falseAlarm.visibility)
        assertEquals(View.VISIBLE, row.missed.visibility)
        row.falseAlarm.performClick()
        assertEquals(emptyList<WakeFeedbackKind>(), marks)
        row.missed.performClick()
        assertEquals(listOf(WakeFeedbackKind.MISSED), marks)
    }
}
