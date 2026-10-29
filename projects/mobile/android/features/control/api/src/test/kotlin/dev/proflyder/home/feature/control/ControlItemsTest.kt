package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ControlItemsTest {
    private val texts = ControlTexts(
        otherDevices = "Other devices",
        scenarios = "Scenarios",
        unavailable = "Unavailable",
        onSummary = { on, total -> "$on of $total on" },
    )

    private fun entry(
        id: String,
        kind: ControlKind = ControlKind.LIGHT,
        room: String = "",
        power: PowerState? = null,
        available: Boolean = true,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = id,
        room = room,
        icon = "",
        actions = if (kind == ControlKind.SCENARIO) setOf(ControlAction.RUN) else setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = available,
    )

    private val sample = listOf(
        entry("scene.movie", ControlKind.SCENARIO),
        entry("light.a", room = "Bedroom", power = PowerState.ON),
        entry("light.b", room = "Bedroom", power = PowerState.OFF),
        entry("switch.fan", ControlKind.SWITCH, room = "Workspace", power = PowerState.OFF),
        entry("light.hall", power = PowerState.ON),
        entry("light.dead", room = "Bedroom", available = false),
    )

    private fun build(
        entries: List<ControlEntry> = sample,
        filter: ControlFilter = ControlFilter.All,
        pending: Set<String> = emptySet(),
        statesOk: Boolean = true,
    ) = ControlItems.build(entries, pending, statesOk, filter, texts)

    private fun List<ControlItem>.describe(): List<String> = map {
        when (it) {
            is ControlItem.Header -> "# ${it.title}" + (it.summary?.let { s -> " ($s)" } ?: "")
            is ControlItem.Scenario -> "S ${it.entry.id}"
            is ControlItem.Device -> "D ${it.entry.id}"
        }
    }

    @Test
    fun optionsListCategoriesThenRoomsWithCounts() {
        val options = ControlItems.options(sample)
        assertEquals(
            listOf(
                ControlFilterOption(ControlFilter.All, 6),
                ControlFilterOption(ControlFilter.Scenarios, 1),
                ControlFilterOption(ControlFilter.Lights, 4),
                ControlFilterOption(ControlFilter.Switches, 1),
                ControlFilterOption(ControlFilter.Room("Bedroom"), 3),
                ControlFilterOption(ControlFilter.Room("Workspace"), 1),
            ),
            options,
        )
    }

    @Test
    fun roomChipsAreOmittedWhenThereIsOnlyOneRoom() {
        val options = ControlItems.options(listOf(entry("light.a", room = "Bedroom"), entry("light.b", room = "Bedroom")))
        assertEquals(listOf(ControlFilter.All, ControlFilter.Lights), options.map { it.filter })
        assertTrue(ControlItems.options(emptyList()).isEmpty())
    }

    @Test
    fun allShowsScenariosThenRoomsWithOnCountsThenUnavailable() {
        assertEquals(
            listOf(
                "# Scenarios", "S scene.movie",
                "# Bedroom (1 of 2 on)", "D light.a", "D light.b",
                "# Workspace (0 of 1 on)", "D switch.fan",
                "# Other devices (1 of 1 on)", "D light.hall",
                "# Unavailable (1)", "D light.dead",
            ),
            build().describe(),
        )
    }

    @Test
    fun scenariosFilterShowsTilesWithoutAHeader() {
        assertEquals(listOf("S scene.movie"), build(filter = ControlFilter.Scenarios).describe())
    }

    @Test
    fun categoryAndRoomFiltersNarrowTheList() {
        assertEquals(
            listOf("# Workspace (0 of 1 on)", "D switch.fan"),
            build(filter = ControlFilter.Switches).describe(),
        )
        assertEquals(
            listOf("# Bedroom (1 of 2 on)", "D light.a", "D light.b", "# Unavailable (1)", "D light.dead"),
            build(filter = ControlFilter.Room("Bedroom")).describe(),
        )
    }

    @Test
    fun onSummaryIsOmittedWhenHomeAssistantStatesAreUnknown() {
        val headers = build(statesOk = false).filterIsInstance<ControlItem.Header>()
        assertNull(headers.first { it.section == "room:Bedroom" }.summary)
        assertFalse(build(statesOk = false).filterIsInstance<ControlItem.Device>().any { it.statesKnown })
    }

    @Test
    fun pendingEntriesAreMarkedBusy() {
        val items = build(pending = setOf("light.a", "scene.movie"))
        assertTrue(items.filterIsInstance<ControlItem.Device>().single { it.entry.id == "light.a" }.busy)
        assertTrue(items.filterIsInstance<ControlItem.Scenario>().single().busy)
        assertFalse(items.filterIsInstance<ControlItem.Device>().single { it.entry.id == "light.b" }.busy)
    }

    @Test
    fun aStaleFilterFallsBackToAllInsteadOfAnEmptyList() {
        val gone = ControlFilter.Room("Garage")
        assertEquals(ControlFilter.All, ControlItems.effectiveFilter(sample, gone))
        assertEquals(build(filter = ControlFilter.All).describe(), build(filter = gone).describe())
        // A category that no longer exists also falls back.
        val onlyLights = listOf(entry("light.a"), entry("light.b"))
        assertEquals(ControlFilter.All, ControlItems.effectiveFilter(onlyLights, ControlFilter.Switches))
        // Still-valid choices are kept, and so is any choice while nothing is loaded yet.
        assertEquals(ControlFilter.Lights, ControlItems.effectiveFilter(sample, ControlFilter.Lights))
        assertEquals(gone, ControlItems.effectiveFilter(emptyList(), gone))
    }

    @Test
    fun aRoomNamedLikeASectionDoesNotCollideWithIt() {
        val entries = listOf(
            entry("scene.movie", ControlKind.SCENARIO),
            entry("light.a", room = "Scenarios"),
            entry("light.b", room = "Unavailable"),
            entry("light.c", room = "Other devices"),
            entry("light.hall"),
            entry("light.dead", available = false),
        )
        val keys = build(entries).map { it.key }
        assertEquals("keys must be unique: $keys", keys.size, keys.toSet().size)
        val headers = build(entries).filterIsInstance<ControlItem.Header>()
        assertEquals(
            listOf("scenarios", "room:Scenarios", "room:Unavailable", "room:Other devices", "other", "unavailable"),
            headers.map { it.section },
        )
    }
}
