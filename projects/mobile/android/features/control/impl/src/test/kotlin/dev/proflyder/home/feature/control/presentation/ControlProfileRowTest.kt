package dev.proflyder.home.feature.control.presentation

import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.impl.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * Robolectric smoke test of the profile row: it must inflate with real
 * resources, show the current side, and report only *user* choices — the
 * programmatic `check` during binding must never send a command.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ControlProfileRowTest {
    private val changes = mutableListOf<Boolean>()
    private val powerChanges = mutableListOf<Boolean>()
    private val rowClicks = mutableListOf<ControlItem>()
    private val settingsToggles = mutableListOf<String>()

    private val callback = object : BaseAdapterCallback<ControlItem>(), ControlAdapterCallback {
        override fun onItemClick(item: ControlItem) {
            rowClicks += item
        }

        override fun onPowerChanged(entry: ControlEntry, on: Boolean) {
            powerChanges += on
        }

        override fun onProfileChanged(entry: ControlEntry, work: Boolean) {
            changes += work
        }

        override fun onToggleSettings(deviceId: String) {
            settingsToggles += deviceId
        }

        override fun onHeaderClick(header: ControlItem.Header) = Unit

        override fun onPinToggle(entry: ControlEntry) = Unit

        override fun onHubCardClick(card: HubCard) = Unit

        override fun onHubPrimary(card: HubCard) = Unit

        override fun onQuickTileClick(tile: QuickTile) = Unit

        override fun onQuickTileLongClick(tile: QuickTile) = Unit

        override fun onFilterSelected(filter: ControlFilter) = Unit
    }

    private fun profile(power: PowerState?, available: Boolean = true) = ControlEntry(
        id = "switch.voice_profile",
        kind = ControlKind.PROFILE,
        title = "Voice profile",
        room = "",
        icon = "account-voice",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = available,
    )

    private fun device(power: PowerState?) = ControlEntry(
        id = "light.desk",
        kind = ControlKind.LIGHT,
        title = "Desk lamp",
        room = "Office",
        icon = "lightbulb-outline",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = true,
    )

    private class Row(
        val root: View,
        val group: MaterialButtonToggleGroup,
        val personal: MaterialButton,
        val work: MaterialButton,
    )

    private fun bind(item: ControlItem.Profile): Row {
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
            holder.itemView.findViewById(R.id.control_profile_group),
            holder.itemView.findViewById(R.id.control_profile_personal),
            holder.itemView.findViewById(R.id.control_profile_work),
        )
    }

    @Test
    fun showsTheCurrentProfileAndReportsOnlyTaps() {
        val off = bind(ControlItem.Profile(profile(PowerState.OFF), busy = false, statesKnown = true))
        assertEquals("Voice profile", off.root.findViewById<android.widget.TextView>(R.id.control_profile_title).text)
        assertEquals("Personal", off.personal.text)
        assertEquals("Work", off.work.text)
        assertTrue(off.personal.isChecked)
        assertFalse(off.work.isChecked)
        // Binding is not a user choice.
        assertEquals(emptyList<Boolean>(), changes)

        off.work.performClick()
        assertEquals(listOf(true), changes)
        off.personal.performClick()
        assertEquals(listOf(true, false), changes)

        val on = bind(ControlItem.Profile(profile(PowerState.ON), busy = false, statesKnown = true))
        assertTrue(on.work.isChecked)
        assertFalse(on.personal.isChecked)
        // Re-binding a row never replays a command.
        assertEquals(listOf(true, false), changes)
    }

    @Test
    fun unknownStateShowsNeitherSideAndNoneIsSent() {
        val row = bind(ControlItem.Profile(profile(null), busy = false, statesKnown = false))
        assertFalse(row.personal.isChecked)
        assertFalse(row.work.isChecked)
        assertEquals(emptyList<Boolean>(), changes)
    }

    @Test
    fun unavailableOrBusyRowCannotBeTapped() {
        val unavailable = bind(ControlItem.Profile(profile(PowerState.OFF, available = false), busy = false, statesKnown = true))
        assertFalse(unavailable.personal.isEnabled)
        assertFalse(unavailable.work.isEnabled)
        unavailable.work.performClick()
        assertEquals(emptyList<Boolean>(), changes)

        val busy = bind(ControlItem.Profile(profile(PowerState.OFF), busy = true, statesKnown = true))
        assertFalse(busy.work.isEnabled)
        assertFalse(busy.personal.isEnabled)
        busy.work.performClick()
        assertEquals(emptyList<Boolean>(), changes)
    }

    @Test
    fun theRowItselfDoesNothing() {
        // There is no "toggle": the card is not clickable and only the two
        // explicit sides send a command.
        val row = bind(ControlItem.Profile(profile(PowerState.OFF), busy = false, statesKnown = true))
        row.root.performClick()
        assertEquals(emptyList<ControlItem>(), rowClicks)
        assertEquals(emptyList<Boolean>(), changes)
    }

    @Test
    fun powerChangeUpdatesTheSameHolderThroughThePayload() {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        val parent = FrameLayout(themed)
        val off = ControlItem.Device(device(PowerState.OFF), busy = false, statesKnown = true)
        val on = ControlItem.Device(device(PowerState.ON), busy = false, statesKnown = true)

        val before = ControlAdapter(callback)
        before.submitList(listOf(off))
        val holder = before.onCreateViewHolder(parent, before.getItemViewType(0))
        before.onBindViewHolder(holder, 0)
        val switch = holder.itemView.findViewById<MaterialSwitch>(R.id.control_device_switch)
        assertFalse(switch.isChecked)

        // The diff reports a typed payload, so the list animator reuses this
        // holder and only the changed views are rebound.
        val payload = requireNotNull(ControlDiff.getChangePayload(off, on))
        val after = ControlAdapter(callback)
        after.submitList(listOf(on))
        after.onBindViewHolder(holder, 0, mutableListOf(payload))

        assertTrue(switch.isChecked)
        val state = holder.itemView.findViewById<android.widget.TextView>(R.id.control_device_state)
        assertEquals("On", state.text.toString())
        // Payload binding is programmatic and must never look like a user toggle.
        assertEquals(emptyList<Boolean>(), powerChanges)
    }

    private fun setting(power: PowerState?) = ControlEntry(
        id = "switch.desk.timer",
        kind = ControlKind.SWITCH,
        title = "Timer",
        room = "Office",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = true,
        deviceId = "dev.desk",
        device = "Desk lamp",
        role = ControlRole.SETTING,
    )

    private fun bindAny(item: ControlItem): View {
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        val parent = FrameLayout(themed)
        val adapter = ControlAdapter(callback)
        adapter.submitList(listOf(item))
        val holder = adapter.onCreateViewHolder(parent, adapter.getItemViewType(0))
        adapter.onBindViewHolder(holder, 0)
        return holder.itemView
    }

    @Test
    fun deviceGroupShowsRowsAndTogglesSettings() {
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.desk",
            title = "Desk lamp",
            primary = listOf(ControlItem.DeviceGroup.Row(device(PowerState.ON), busy = false)),
            settings = listOf(ControlItem.DeviceGroup.Row(setting(PowerState.OFF), busy = false)),
            expanded = false,
            statesKnown = true,
        )
        val root = bindAny(group)
        // A single-primary card has no separate header: its row is the title.
        assertEquals(
            View.GONE,
            root.findViewById<android.widget.TextView>(R.id.control_group_title).visibility,
        )
        val primary = root.findViewById<LinearLayout>(R.id.control_group_primary)
        assertEquals(1, primary.childCount)
        assertEquals(
            "Desk lamp",
            primary.getChildAt(0).findViewById<android.widget.TextView>(R.id.control_row_title).text.toString(),
        )
        val settingsButton = root.findViewById<MaterialButton>(R.id.control_group_settings)
        assertEquals("Settings (1)", settingsButton.text.toString())
        assertEquals(View.VISIBLE, settingsButton.visibility)
        assertEquals(View.GONE, root.findViewById<LinearLayout>(R.id.control_group_settings_list).visibility)
        settingsButton.performClick()
        assertEquals(listOf("dev.desk"), settingsToggles)

        val expanded = bindAny(group.copy(expanded = true))
        assertEquals(
            View.VISIBLE,
            expanded.findViewById<LinearLayout>(R.id.control_group_settings_list).visibility,
        )
        assertEquals(1, expanded.findViewById<LinearLayout>(R.id.control_group_settings_list).childCount)
    }

    @Test
    fun multiPrimaryGroupKeepsTheHeaderAndShortRowNames() {
        fun primary(id: String, title: String) = ControlItem.DeviceGroup.Row(
            ControlEntry(
                id = id,
                kind = ControlKind.SWITCH,
                title = title,
                room = "Living room",
                icon = "",
                actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
                power = PowerState.OFF,
                available = true,
                deviceId = "dev.speaker",
                device = "Колонка",
            ),
            busy = false,
        )
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.speaker",
            title = "Колонка",
            primary = listOf(
                primary("media.mic", "Microphones"),
                primary("media.clock", "Clock display"),
                primary("media.ring", "Ring"),
            ),
            settings = emptyList(),
            expanded = false,
            statesKnown = true,
        )
        val root = bindAny(group)
        val header = root.findViewById<android.widget.TextView>(R.id.control_group_title)
        assertEquals(View.VISIBLE, header.visibility)
        assertEquals("Колонка", header.text.toString())
        val container = root.findViewById<LinearLayout>(R.id.control_group_primary)
        val titles = (0 until container.childCount).map {
            container.getChildAt(it).findViewById<android.widget.TextView>(R.id.control_row_title).text.toString()
        }
        assertEquals(listOf("Microphones", "Clock display", "Ring"), titles)
    }
}
