package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.CatalogGrouping
import dev.proflyder.home.core.model.CatalogGroupingLabels
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.PowerState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class CatalogGroupingTest {
    private val labels = object : CatalogGroupingLabels {
        override fun scenariosHa() = "Scenarios HA"
        override fun other() = "Other"
        override fun light() = "Light"
        override fun switch() = "Switch"
        override fun scenario() = "Scenario"
        override fun action() = "Action"
        override fun device() = "Device"
    }

    private fun entry(
        id: String,
        kind: ControlKind = ControlKind.SWITCH,
        title: String = id,
        device: String = "",
        room: String = "",
        role: ControlRole = ControlRole.PRIMARY,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = title,
        room = room,
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.OFF,
        available = true,
        device = device,
        role = role,
    )

    @Test
    fun groupsWithoutADeviceUseHumanTitlesAndNeverRawIds() {
        val rooms = CatalogGrouping.rooms(
            entries = listOf(
                entry("switch.komp", title = "switch.komp"),
                entry("script.wake_false_alarm", ControlKind.SCENARIO, title = "script.wake_false_alarm"),
            ),
            includeSettings = false,
            labels = labels,
        )
        val titles = rooms.flatMap { room -> room.devices.map { it.title } }
        assertEquals(listOf("Other", "Scenarios HA"), titles)
        assertFalse(titles.any { it.contains('.') })
    }

    @Test
    fun aMeaningfulTitleNamesItsOwnGroup() {
        val room = CatalogGrouping.rooms(
            listOf(entry("switch.scene", title = "Wake-up light", device = "", room = "Bedroom")),
            includeSettings = false,
            labels = labels,
        ).single()
        assertEquals("Wake-up light", room.devices.single().title)
        assertFalse(CatalogGrouping.needsDeviceHeader(room.devices.single(), labels))
    }

    @Test
    fun aSingleRowRepeatingTheGroupTitleNeedsNoSubheader() {
        val group = CatalogGrouping.rooms(
            listOf(entry("light.lamp", ControlKind.LIGHT, title = "Light", device = "Лампа")),
            includeSettings = false,
            labels = labels,
        ).single().devices.single()
        assertEquals("Лампа", group.title)
        assertEquals("Лампа", CatalogGrouping.entryLabel(group.entries.single(), labels))
        assertFalse(CatalogGrouping.needsDeviceHeader(group, labels))
    }

    @Test
    fun serviceSettingsAreHiddenButCounted() {
        val entries = listOf(
            entry("light.a", ControlKind.LIGHT, title = "Light", device = "Лампа"),
            entry("switch.a.delay", title = "delay-off-enable", device = "Лампа", role = ControlRole.SETTING),
            entry("switch.a.save", title = "save-state", device = "Лампа", role = ControlRole.SETTING),
        )
        val hidden = CatalogGrouping.rooms(entries, includeSettings = false, labels = labels).single().devices.single()
        assertEquals(listOf("light.a"), hidden.entries.map { it.id })
        assertEquals(2, CatalogGrouping.settingsCount(entries))
        val shown = CatalogGrouping.rooms(entries, includeSettings = true, labels = labels).single().devices.single()
        assertEquals(3, shown.entries.size)
        assertEquals("light.a", shown.entries.first().id)
    }

    @Test
    fun technicalRowTitlesFallBackToTheKindNameNotTheId() {
        val entry = entry("switch.komp", title = "switch.komp")
        assertEquals("Switch", CatalogGrouping.entryLabel(entry, labels))
    }
}
