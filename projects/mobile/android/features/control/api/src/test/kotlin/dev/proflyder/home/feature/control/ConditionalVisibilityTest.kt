package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.VisibleWhen
import dev.proflyder.home.core.model.VolumeControls
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ConditionalVisibilityTest {
    private val texts = ControlTexts(
        otherDevices = "Other",
        scenarios = "Scenarios",
        unavailable = "Unavailable",
        voice = "Voice",
        onSummary = { on, total -> "$on of $total on" },
    )

    private fun entry(
        id: String,
        kind: ControlKind,
        power: PowerState?,
        visibleWhen: VisibleWhen? = null,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = id,
        room = "Office",
        icon = "",
        actions = if (kind == ControlKind.SWITCH) {
            setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)
        } else {
            setOf(ControlAction.PRESS)
        },
        power = power,
        available = true,
        visibleWhen = visibleWhen,
    )

    private fun build(basePower: PowerState?): List<ControlItem> = ControlItems.build(
        entries = listOf(
            entry("switch.komp", ControlKind.SWITCH, basePower),
            entry(
                "toggle.komp_lock",
                ControlKind.SWITCH,
                PowerState.ON,
                VisibleWhen(entry = "switch.komp", state = "on"),
            ),
        ),
        pending = emptySet(),
        statesOk = true,
        filter = ControlFilter.All,
        texts = texts,
    )

    private fun ids(items: List<ControlItem>): List<String> = items.flatMap { item ->
        when (item) {
            is ControlItem.Device -> listOf(item.entry.id)
            is ControlItem.DeviceGroup -> item.primary.map { it.entry.id }
            else -> emptyList()
        }
    }

    @Test
    fun aConditionalEntryIsHiddenWhileItsBaseIsOffOrUnknown() {
        val off = entry("switch.komp", ControlKind.SWITCH, PowerState.OFF)
        val unknown = entry("switch.komp", ControlKind.SWITCH, null)
        val lock = entry(
            "toggle.komp_lock",
            ControlKind.SWITCH,
            PowerState.ON,
            VisibleWhen(entry = "switch.komp", state = "on"),
        )
        assertFalse(lock.isVisible(listOf(off, lock)))
        assertFalse(lock.isVisible(listOf(unknown, lock)))
        assertTrue(lock.isVisible(listOf(entry("switch.komp", ControlKind.SWITCH, PowerState.ON), lock)))
    }

    @Test
    fun theLockToggleAppearsAndDisappearsWithTheCardMainSwitch() {
        val hidden = build(PowerState.OFF)
        assertEquals(listOf("switch.komp"), ids(hidden))
        val shown = build(PowerState.ON)
        assertEquals(listOf("switch.komp", "toggle.komp_lock"), ids(shown))
        val group = shown.filterIsInstance<ControlItem.DeviceGroup>().single()
        assertEquals("switch.komp", group.mainId)
        val lock = group.primary.single { it.entry.id == "toggle.komp_lock" }.entry
        assertTrue(lock.isCompactTile(group.primary.size, isMain = false))
    }

    @Test
    fun aSinglePrimaryControlWithSettingsStaysTheMainRow() {
        val lamp = ControlEntry(
            id = "light.lamp",
            kind = ControlKind.LIGHT,
            title = "Lamp",
            room = "Office",
            icon = "",
            actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
            power = PowerState.ON,
            available = true,
            deviceId = "dev.lamp",
            device = "Lamp",
        )
        val timer = lamp.copy(
            id = "switch.lamp_timer",
            kind = ControlKind.SWITCH,
            title = "Timer",
            role = ControlRole.SETTING,
        )
        val group = ControlItems.build(
            entries = listOf(lamp, timer),
            pending = emptySet(),
            statesOk = true,
            filter = ControlFilter.All,
            texts = texts,
        ).filterIsInstance<ControlItem.DeviceGroup>().single()
        assertEquals("light.lamp", group.mainId)
        assertFalse(lamp.copy(title = "Lamp").isCompactTile(2, isMain = true))
    }

    @Test
    fun volumeEntriesOfOneDeviceBecomeOneRowWithACompanion() {
        val main = ControlEntry(
            id = "volume.station",
            kind = ControlKind.VOLUME,
            title = "Volume",
            room = "Bedroom",
            icon = "",
            actions = setOf(ControlAction.SET, ControlAction.MUTE, ControlAction.UNMUTE),
            power = null,
            available = true,
            volume = VolumeControls(level = 10),
            deviceId = "dev.station",
            device = "Station",
        )
        val announcement = main.copy(
            id = "volume.station_announce",
            title = "Announcement volume",
            actions = setOf(ControlAction.SET),
            volume = VolumeControls(level = 100),
        )
        val items = ControlItems.build(
            entries = listOf(main, announcement),
            pending = emptySet(),
            statesOk = true,
            filter = ControlFilter.All,
            texts = texts,
        )
        val row = items.filterIsInstance<ControlItem.Device>().single().entry
        assertEquals("volume.station", row.id)
        assertEquals("volume.station_announce", row.volumeCompanion?.id)
    }

    @Test
    fun aStatusDetailDoesNotPromoteAStationSwitchToTheMainRow() {
        fun stationEntry(id: String, title: String, detail: String = "", volume: VolumeControls? = null) =
            ControlEntry(
                id = id,
                kind = if (volume != null) ControlKind.VOLUME else ControlKind.SWITCH,
                title = title,
                room = "Bedroom",
                icon = "",
                actions = if (volume != null) {
                    setOf(ControlAction.SET, ControlAction.MUTE, ControlAction.UNMUTE)
                } else {
                    setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)
                },
                power = if (volume != null) null else PowerState.ON,
                available = true,
                detailKey = detail,
                volume = volume,
                deviceId = "dev.station",
                device = "Yandex Station Midi",
            )
        val group = ControlItems.build(
            entries = listOf(
                stationEntry("volume.station", "Volume", volume = VolumeControls(level = 10)),
                stationEntry("switch.mic", "Microphones", detail = "catalog.status.muted"),
                stationEntry("light.backlight", "Backlight"),
                stationEntry("switch.clock", "Clock"),
            ),
            pending = emptySet(),
            statesOk = true,
            filter = ControlFilter.All,
            texts = texts,
        ).filterIsInstance<ControlItem.DeviceGroup>().single()
        assertNull(group.mainId)
        val binaries = group.primary.filter { !it.entry.isVolume }
        assertEquals(3, binaries.size)
        assertTrue(binaries.all { it.entry.isCompactTile(group.primary.size) })
        assertFalse(group.primary.single { it.entry.id == "volume.station" }.entry.isCompactTile(group.primary.size))
    }
}
