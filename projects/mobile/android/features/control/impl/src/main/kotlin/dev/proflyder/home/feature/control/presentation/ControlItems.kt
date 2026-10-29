package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.ControlFilter

/** Rows of the Home screen: section headers, scenario tiles and device rows. */
internal sealed interface ControlItem {
    /** Stable identity for DiffUtil. */
    val key: String

    /** [summary] is a short right-aligned note such as "2 of 4 on"; null for none. */
    data class Header(val title: String, val summary: String? = null) : ControlItem {
        override val key: String get() = "header:$title"
    }

    data class Scenario(val entry: ControlEntry, val busy: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    data class Device(val entry: ControlEntry, val busy: Boolean, val statesKnown: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }
}

/** A selectable chip: the filter plus how many entries it would show. */
internal data class ControlFilterOption(val filter: ControlFilter, val count: Int)

/** Texts the list needs; the presentation layer supplies localized strings. */
internal class ControlTexts(
    val otherDevices: String,
    val scenarios: String,
    val unavailable: String,
    val onSummary: (on: Int, total: Int) -> String,
)

internal object ControlItems {
    private fun ControlEntry.matches(filter: ControlFilter): Boolean = when (filter) {
        ControlFilter.All -> true
        ControlFilter.Scenarios -> kind == ControlKind.SCENARIO
        ControlFilter.Lights -> kind == ControlKind.LIGHT
        ControlFilter.Switches -> kind == ControlKind.SWITCH
        is ControlFilter.Room -> room == filter.name
    }

    /**
     * The chips: All, then the categories that exist, then each room (only when
     * there is more than one, otherwise rooms add nothing).
     */
    fun options(entries: List<ControlEntry>): List<ControlFilterOption> {
        if (entries.isEmpty()) return emptyList()
        val options = mutableListOf(ControlFilterOption(ControlFilter.All, entries.size))
        listOf(ControlFilter.Scenarios, ControlFilter.Lights, ControlFilter.Switches).forEach { filter ->
            val count = entries.count { it.matches(filter) }
            if (count > 0) options += ControlFilterOption(filter, count)
        }
        val rooms = entries.map { it.room }.filter { it.isNotBlank() }.distinct()
        if (rooms.size > 1) {
            rooms.forEach { room -> options += ControlFilterOption(ControlFilter.Room(room), entries.count { it.room == room }) }
        }
        return options
    }

    /**
     * Scenarios first (one section, headed unless that is the only thing shown),
     * then available devices grouped by room, then unavailable devices in their
     * own trailing section so dead entries do not interleave with working ones.
     */
    fun build(state: ControlUiState, filter: ControlFilter, texts: ControlTexts): List<ControlItem> {
        val visible = state.entries.filter { it.matches(filter) }
        val items = mutableListOf<ControlItem>()

        val scenarios = visible.filter { it.kind == ControlKind.SCENARIO }
        if (scenarios.isNotEmpty()) {
            if (filter != ControlFilter.Scenarios) items += ControlItem.Header(texts.scenarios)
            scenarios.mapTo(items) { ControlItem.Scenario(it, it.id in state.pending) }
        }

        val devices = visible.filter { it.isDevice }
        val (working, dead) = devices.partition { it.available }
        val byRoom = LinkedHashMap<String, MutableList<ControlEntry>>()
        working.forEach { byRoom.getOrPut(it.room.ifBlank { texts.otherDevices }) { mutableListOf() } += it }
        byRoom.forEach { (room, entries) ->
            val on = entries.count { it.power == PowerState.ON }
            val summary = if (state.statesOk) texts.onSummary(on, entries.size) else null
            items += ControlItem.Header(room, summary)
            entries.mapTo(items) { ControlItem.Device(it, it.id in state.pending, state.statesOk) }
        }
        if (dead.isNotEmpty()) {
            items += ControlItem.Header(texts.unavailable, dead.size.toString())
            dead.mapTo(items) { ControlItem.Device(it, it.id in state.pending, state.statesOk) }
        }
        return items
    }
}
