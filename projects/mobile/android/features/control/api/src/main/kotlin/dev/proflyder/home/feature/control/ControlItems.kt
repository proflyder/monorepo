package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState

/*
 * Pure presentation logic of the Home screen (no Android types), kept here so it
 * is unit-tested: which filter chips exist and how entries become list rows.
 */

/** Rows of the Home screen: section headers, scenario tiles and device rows. */
sealed interface ControlItem {
    /** Stable identity for DiffUtil; unique across item types and sections. */
    val key: String

    /**
     * A section header. [section] identifies the section (not its title), so a
     * room that happens to be called "Scenarios" never collides with the
     * scenarios section. [summary] is a short right-aligned note ("2 of 4 on").
     */
    data class Header(val section: String, val title: String, val summary: String? = null) : ControlItem {
        override val key: String get() = "header:$section"
    }

    data class Scenario(val entry: ControlEntry, val busy: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    data class Device(val entry: ControlEntry, val busy: Boolean, val statesKnown: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }
}

/** A selectable chip: the filter plus how many entries it shows. */
data class ControlFilterOption(val filter: ControlFilter, val count: Int)

/** Texts the list needs; the presentation layer supplies localized strings. */
class ControlTexts(
    val otherDevices: String,
    val scenarios: String,
    val unavailable: String,
    val onSummary: (on: Int, total: Int) -> String,
)

object ControlItems {
    private const val SECTION_SCENARIOS = "scenarios"
    private const val SECTION_UNAVAILABLE = "unavailable"
    private const val SECTION_OTHER = "other"
    private const val SECTION_ROOM_PREFIX = "room:"

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
            rooms.forEach { room ->
                options += ControlFilterOption(ControlFilter.Room(room), entries.count { it.room == room })
            }
        }
        return options
    }

    /**
     * The filter to actually apply: [filter] if its chip exists for [entries],
     * otherwise [ControlFilter.All] (the selected room vanished, or a stored
     * choice no longer matches). With no entries there is nothing to validate
     * against yet, so [filter] is kept.
     */
    fun effectiveFilter(entries: List<ControlEntry>, filter: ControlFilter): ControlFilter =
        if (entries.isEmpty() || options(entries).any { it.filter == filter }) filter else ControlFilter.All

    /**
     * Scenarios first (one section, headed unless that is the only thing shown),
     * then available devices grouped by room, then unavailable devices in their
     * own trailing section so dead entries do not interleave with working ones.
     */
    fun build(
        entries: List<ControlEntry>,
        pending: Set<String>,
        statesOk: Boolean,
        filter: ControlFilter,
        texts: ControlTexts,
    ): List<ControlItem> {
        val applied = effectiveFilter(entries, filter)
        val visible = entries.filter { it.matches(applied) }
        val items = mutableListOf<ControlItem>()

        val scenarios = visible.filter { it.kind == ControlKind.SCENARIO }
        if (scenarios.isNotEmpty()) {
            if (applied != ControlFilter.Scenarios) {
                items += ControlItem.Header(SECTION_SCENARIOS, texts.scenarios)
            }
            scenarios.mapTo(items) { ControlItem.Scenario(it, it.id in pending) }
        }

        val devices = visible.filter { it.isDevice }
        val (working, dead) = devices.partition { it.available }
        val byRoom = LinkedHashMap<String, MutableList<ControlEntry>>()
        working.forEach { byRoom.getOrPut(it.room) { mutableListOf() } += it }
        byRoom.forEach { (room, roomEntries) ->
            val on = roomEntries.count { it.power == PowerState.ON }
            val summary = if (statesOk) texts.onSummary(on, roomEntries.size) else null
            items += if (room.isBlank()) {
                ControlItem.Header(SECTION_OTHER, texts.otherDevices, summary)
            } else {
                ControlItem.Header(SECTION_ROOM_PREFIX + room, room, summary)
            }
            roomEntries.mapTo(items) { ControlItem.Device(it, it.id in pending, statesOk) }
        }
        if (dead.isNotEmpty()) {
            items += ControlItem.Header(SECTION_UNAVAILABLE, texts.unavailable, dead.size.toString())
            dead.mapTo(items) { ControlItem.Device(it, it.id in pending, statesOk) }
        }
        return items
    }
}
