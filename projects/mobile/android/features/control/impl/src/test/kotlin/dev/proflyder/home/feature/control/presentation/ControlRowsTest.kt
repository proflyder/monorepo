package dev.proflyder.home.feature.control.presentation

import android.graphics.drawable.RippleDrawable
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.button.MaterialButton
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.designsystem.TileGrid
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceTileBinding
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ControlRowsTest {
    private val intents = mutableListOf<ControlIntent>()

    private fun device(power: PowerState?, light: LightControls? = null) = ControlEntry(
        id = "light.desk",
        kind = ControlKind.LIGHT,
        title = "Desk lamp",
        room = "Office",
        icon = "lightbulb-outline",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = true,
        light = light,
    )

    private fun themed() = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
    )

    @Test
    fun powerChangeRendersFromStateAndATapOnlySendsAnIntent() {
        val off = ControlItem.Device(device(PowerState.OFF), busy = false, statesKnown = true)
        val on = ControlItem.Device(device(PowerState.ON), busy = false, statesKnown = true)
        val adapter = ControlAdapter(ControlAdapterCallback(intents::add))
        adapter.submitList(listOf(off))
        val holder = adapter.onCreateViewHolder(FrameLayout(themed()), adapter.getItemViewType(0))
        adapter.onBindViewHolder(holder, 0)
        val switch = holder.itemView.findViewById<MaterialSwitch>(R.id.control_device_switch)
        assertFalse(switch.isChecked)

        switch.performClick()
        assertFalse("the switch waits for the state", switch.isChecked)
        assertEquals(listOf(ControlIntent.Command(DeviceCommand.Power(off.entry, true))), intents)

        val payload = requireNotNull(ControlDiff.getChangePayload(off, on))
        val after = ControlAdapter(ControlAdapterCallback(intents::add))
        after.submitList(listOf(on))
        after.onBindViewHolder(holder, 0, mutableListOf(payload))
        assertTrue(switch.isChecked)
        assertEquals("On", holder.itemView.findViewById<TextView>(R.id.control_device_state).text.toString())
        assertEquals(1, intents.size)
    }

    @Test
    fun aRowTapAndHoldBecomeIntents() {
        val item = ControlItem.Device(device(PowerState.OFF), busy = false, statesKnown = true)
        val root = bindAny(item)
        root.performClick()
        root.performLongClick()
        assertEquals(
            listOf(ControlIntent.Tap(TapTarget.Item(item)), ControlIntent.Hold(TapTarget.Item(item))),
            intents,
        )
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

    private fun buttonSetting() = ControlEntry(
        id = "button.desk_restart",
        kind = ControlKind.BUTTON,
        title = "Restart",
        room = "Office",
        icon = "",
        actions = setOf(ControlAction.PRESS),
        power = null,
        available = true,
        deviceId = "dev.desk",
        device = "Desk lamp",
        role = ControlRole.SETTING,
    )

    private fun bindAny(item: ControlItem): View {
        val parent = FrameLayout(themed())
        val adapter = ControlAdapter(ControlAdapterCallback(intents::add))
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
            primary = listOf(
                ControlItem.DeviceGroup.Row(
                    device(PowerState.ON, LightControls(brightness = true, colorTemp = false, color = false)),
                    busy = false,
                ),
            ),
            settings = listOf(ControlItem.DeviceGroup.Row(setting(PowerState.OFF), busy = false)),
            expanded = false,
            statesKnown = true,
            mainId = "light.desk",
        )
        val root = bindAny(group)
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
        assertEquals(View.GONE, root.findViewById<View>(R.id.control_group_settings_tiles).visibility)
        assertEquals(View.GONE, root.findViewById<LinearLayout>(R.id.control_group_settings_list).visibility)
        settingsButton.performClick()
        assertEquals(listOf(ControlIntent.ToggleDevice("dev.desk")), intents)

        val expanded = bindAny(group.copy(expanded = true))
        val settingsTiles = expanded.findViewById<TileGrid>(R.id.control_group_settings_tiles)
        assertEquals(View.VISIBLE, settingsTiles.visibility)
        assertEquals(1, settingsTiles.childCount)
        assertEquals(View.GONE, expanded.findViewById<LinearLayout>(R.id.control_group_settings_list).visibility)
    }

    @Test
    fun multiPrimaryGroupKeepsTheHeaderAndShowsCompactTiles() {
        fun primary(id: String, title: String, power: PowerState?, available: Boolean = true) =
            ControlItem.DeviceGroup.Row(
                ControlEntry(
                    id = id,
                    kind = ControlKind.SWITCH,
                    title = title,
                    room = "Living room",
                    icon = "",
                    actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
                    power = power,
                    available = available,
                    deviceId = "dev.speaker",
                    device = "Колонка",
                ),
                busy = false,
            )
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.speaker",
            title = "Колонка",
            primary = listOf(
                primary("media.mic", "Microphones", PowerState.ON),
                primary("media.clock", "Clock display", PowerState.OFF),
                primary("media.ring", "Ring", PowerState.ON, available = false),
            ),
            settings = emptyList(),
            expanded = false,
            statesKnown = true,
        )
        val root = bindAny(group)
        val header = root.findViewById<android.widget.TextView>(R.id.control_group_title)
        assertEquals(View.VISIBLE, header.visibility)
        assertEquals("Колонка", header.text.toString())
        assertEquals(View.GONE, root.findViewById<LinearLayout>(R.id.control_group_primary).visibility)
        val row = root.findViewById<TileGrid>(R.id.control_group_tiles)
        assertEquals(View.VISIBLE, row.visibility)
        val labels = (0 until 3).map {
            row.getChildAt(it).findViewById<android.widget.TextView>(R.id.control_tile_label).text.toString()
        }
        assertEquals(listOf("Microphones", "Clock display", "Ring"), labels)
        assertEquals("Microphones, On", row.getChildAt(0).contentDescription)
        assertEquals("Clock display, Off", row.getChildAt(1).contentDescription)
        assertEquals("Ring, Unavailable", row.getChildAt(2).contentDescription)
    }

    @Test
    fun rebindingAGroupKeepsTheRowViews() {
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.desk",
            title = "Desk lamp",
            primary = listOf(ControlItem.DeviceGroup.Row(device(PowerState.ON), busy = false)),
            settings = listOf(ControlItem.DeviceGroup.Row(buttonSetting(), busy = false)),
            expanded = true,
            statesKnown = true,
        )
        val themed = ContextThemeWrapper(
            RuntimeEnvironment.getApplication(),
            com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
        )
        val adapter = ControlAdapter(ControlAdapterCallback(intents::add))
        adapter.submitList(listOf(group))
        val holder = adapter.onCreateViewHolder(FrameLayout(themed), adapter.getItemViewType(0))
        adapter.onBindViewHolder(holder, 0)
        val settingsList = holder.itemView.findViewById<LinearLayout>(R.id.control_group_settings_list)
        val before = settingsList.getChildAt(0)

        val busy = ControlAdapter(ControlAdapterCallback(intents::add))
        busy.submitList(
            listOf(
                group.copy(
                    settings = listOf(ControlItem.DeviceGroup.Row(buttonSetting(), busy = true)),
                ),
            ),
        )
        busy.onBindViewHolder(holder, 0)

        assertTrue(before === settingsList.getChildAt(0))
        assertEquals(View.VISIBLE, before.findViewById<View>(R.id.control_row_progress).visibility)
        assertFalse(before.isEnabled)
    }

    private fun switchEntry(id: String, title: String) = ControlEntry(
        id = id,
        kind = ControlKind.SWITCH,
        title = title,
        room = "Office",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.ON,
        available = true,
        deviceId = "dev.tiles",
        device = "Tiles",
    )

    @Test
    fun aTileRippleIsAClippedOval() {
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.tiles",
            title = "Tiles",
            primary = listOf(
                ControlItem.DeviceGroup.Row(switchEntry("switch.one", "One"), busy = false),
                ControlItem.DeviceGroup.Row(switchEntry("switch.two", "Two"), busy = false),
            ),
            settings = emptyList(),
            expanded = false,
            statesKnown = true,
        )
        val root = bindAny(group)
        val tiles = root.findViewById<TileGrid>(R.id.control_group_tiles)
        val tile = ItemControlDeviceTileBinding.bind(tiles.getChildAt(0))
        assertTrue(tile.controlTileRipple.foreground is RippleDrawable)
        assertTrue(tile.controlTileRipple.clipToOutline)
        assertSame(OvalTileOutlineProvider, tile.controlTileRipple.outlineProvider)
    }

    @Test
    fun aMergedVolumeRowShowsTheAnnouncementLevel() {
        val announce = ControlEntry(
            id = "volume.station_announce",
            kind = ControlKind.VOLUME,
            title = "Announcement volume",
            room = "Bedroom",
            icon = "",
            actions = setOf(ControlAction.SET),
            power = null,
            available = true,
            volume = VolumeControls(level = 100),
            deviceId = "dev.station",
            device = "Station",
        )
        val main = announce.copy(
            id = "volume.station",
            title = "Volume",
            actions = setOf(ControlAction.SET, ControlAction.MUTE, ControlAction.UNMUTE),
            volume = VolumeControls(level = 10),
            volumeCompanion = announce,
        )
        val context = RuntimeEnvironment.getApplication()
        val unavailable = R.string.control_state_unavailable
        assertEquals("10% · announcements 100%", main.localizedVolumeText(context, unavailable))
        assertEquals(
            "Sound off · announcements 100%",
            main.copy(volume = VolumeControls(level = 10, muted = true)).localizedVolumeText(context, unavailable),
        )
    }

    private fun topIn(root: View, view: View): Int {
        var y = 0
        var current: View = view
        while (current !== root) {
            y += current.top
            current = current.parent as View
        }
        return y
    }

    @Test
    fun settingsButtonGapsStayWithinFourteenDp() {
        val group = ControlItem.DeviceGroup(
            deviceId = "dev.tiles",
            title = "Tiles",
            primary = listOf(
                ControlItem.DeviceGroup.Row(switchEntry("switch.one", "One"), busy = false),
                ControlItem.DeviceGroup.Row(switchEntry("switch.two", "Two"), busy = false),
            ),
            settings = listOf(ControlItem.DeviceGroup.Row(setting(PowerState.OFF), busy = false)),
            expanded = false,
            statesKnown = true,
        )
        val root = bindAny(group)
        root.measure(
            View.MeasureSpec.makeMeasureSpec(1000, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        root.layout(0, 0, 1000, root.measuredHeight)

        val button = root.findViewById<MaterialButton>(R.id.control_group_settings)
        val layout = requireNotNull(button.layout)
        val buttonTop = topIn(root, button)
        val textTop = buttonTop + (button.height - layout.height) / 2f + layout.getLineTop(0)
        val textBottom = buttonTop + (button.height - layout.height) / 2f + layout.getLineBottom(0)
        val limit = 14 * root.resources.displayMetrics.density

        assertTrue("text bottom to card bottom", root.height - textBottom <= limit)

        val tiles = root.findViewById<TileGrid>(R.id.control_group_tiles)
        val label = tiles.getChildAt(0).findViewById<TextView>(R.id.control_tile_label)
        val labelBottom = topIn(root, label) + label.height
        assertTrue("tile label to settings text", textTop - labelBottom <= limit)
    }
}
