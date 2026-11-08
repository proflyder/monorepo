package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.PowerState

/*
 * Pure presentation logic of the Home screen (no Android types), kept here so it
 * is unit-tested: which filter chips exist and how entries become list rows.
 */

/**
 * Rows of the Home screen: a section header, the profile selector, scenario
 * tiles, plain device rows and device groups (a device with several entries).
 */
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

    /**
     * The assistant profile selector, pinned above everything else. It is a
     * global mode (`on`/`off` as reported by the server: work / personal), so it
     * gets its own segmented row instead of a device switch.
     */
    data class Profile(val entry: ControlEntry, val busy: Boolean, val statesKnown: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    data class Scenario(val entry: ControlEntry, val busy: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    data class Device(val entry: ControlEntry, val busy: Boolean, val statesKnown: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    /**
     * One device with several controllable entries. [primary] are the rows shown
     * on the card, [settings] sit behind the "Settings (N)" row when [expanded]
     * (owned by the view model), and [actions] is reserved for action entries
     * the server will add later (empty today).
     */
    data class DeviceGroup(
        val deviceId: String,
        val title: String,
        val primary: List<Row>,
        val settings: List<Row>,
        val actions: List<Row> = emptyList(),
        val expanded: Boolean,
        val statesKnown: Boolean,
    ) : ControlItem {
        override val key: String get() = "device:$deviceId"

        data class Row(val entry: ControlEntry, val busy: Boolean)
    }
}

/** A selectable chip: the filter plus how many entries it shows. */
data class ControlFilterOption(val filter: ControlFilter, val count: Int)

/** Texts the list needs; the presentation layer supplies localized strings. */
class ControlTexts(
    val otherDevices: String,
    val scenarios: String,
    val unavailable: String,
    val profile: String,
    val onSummary: (on: Int, total: Int) -> String,
    /** Header of the device-level action cards ("Say on the station"). */
    val actions: String = "Actions",
)

object ControlItems {
    private const val SECTION_PROFILE = "profile"
    private const val SECTION_SCENARIOS = "scenarios"
    private const val SECTION_ACTIONS = "actions"
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
     * The chip counts describe primary controls only: a device's settings are
     * part of it and never add a chip or a count of their own.
     */
    private fun ControlEntry.isCounted(): Boolean = !isProfile && role != ControlRole.SETTING

    /**
     * The chips: All, then the categories that exist, then each room (only when
     * there is more than one, otherwise rooms add nothing). The profile selector
     * is pinned and always shown, so it is never a chip and never counted: the
     * counts describe filterable content only.
     */
    fun options(entries: List<ControlEntry>): List<ControlFilterOption> {
        if (entries.isEmpty()) return emptyList()
        val counted = entries.filter { it.isCounted() }
        val options = mutableListOf(ControlFilterOption(ControlFilter.All, counted.size))
        listOf(ControlFilter.Scenarios, ControlFilter.Lights, ControlFilter.Switches).forEach { filter ->
            val count = counted.count { it.matches(filter) }
            if (count > 0) options += ControlFilterOption(filter, count)
        }
        val rooms = counted.map { it.room }.filter { it.isNotBlank() }.distinct()
        if (rooms.size > 1) {
            rooms.forEach { room ->
                options += ControlFilterOption(ControlFilter.Room(room), counted.count { it.room == room })
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
     * The profile first (one pinned section: it is a mode, not content and is
     * shown whatever the filter is), then scenarios in one section (headed
     * unless that is the only thing shown), then available devices grouped by
     * room and, inside a room, by device, then unavailable devices in their own
     * trailing section so dead entries do not interleave with working ones.
     *
     * [expandedDevices] holds the device ids whose "Settings (N)" row is open.
     */
    fun build(
        entries: List<ControlEntry>,
        pending: Set<String>,
        statesOk: Boolean,
        filter: ControlFilter,
        texts: ControlTexts,
        expandedDevices: Set<String> = emptySet(),
    ): List<ControlItem> {
        val applied = effectiveFilter(entries, filter)
        val profiles = entries.filter { it.isProfile }
        val content = entries.filterNot { it.isProfile }
        val items = mutableListOf<ControlItem>()

        if (profiles.isNotEmpty()) {
            items += ControlItem.Header(SECTION_PROFILE, texts.profile)
            profiles.mapTo(items) { ControlItem.Profile(it, it.id in pending, statesOk) }
        }

        val scenarios = content.filter { it.kind == ControlKind.SCENARIO && it.matches(applied) }
        if (scenarios.isNotEmpty()) {
            if (applied != ControlFilter.Scenarios) {
                items += ControlItem.Header(SECTION_SCENARIOS, texts.scenarios)
            }
            scenarios.mapTo(items) { ControlItem.Scenario(it, it.id in pending) }
        }

        // Device-level actions ("say this text on the station") are one-shot
        // tiles like scenarios, but a separate section: they belong to a
        // device, not to the user's scenario presets.
        val actions = content.filter { it.kind == ControlKind.ACTION && it.matches(applied) }
        if (actions.isNotEmpty()) {
            items += ControlItem.Header(SECTION_ACTIONS, texts.actions)
            actions.mapTo(items) { ControlItem.Scenario(it, it.id in pending) }
        }

        val devices = content.filter { it.isDevice }
        val (working, deadEntries) = devices.partition { it.available }
        // Dead rows keep the old behaviour: they follow the applied filter too,
        // and a lone setting never shows up as its own unavailable device.
        val dead = deadEntries.filter { applied == ControlFilter.All || (it.role != ControlRole.SETTING && it.matches(applied)) }
        val byRoom = LinkedHashMap<String, MutableList<ControlEntry>>()
        working.forEach { byRoom.getOrPut(it.room) { mutableListOf() } += it }
        byRoom.forEach { (room, roomEntries) ->
            // A group is shown when one of its primary controls matches the
            // filter; its settings then follow the device, never the chip.
            val groups = groupByDevice(roomEntries).filter { bucket ->
                applied == ControlFilter.All || bucket.primary.any { it.matches(applied) }
            }
            if (groups.isEmpty()) return@forEach
            val on = groups.sumOf { bucket -> bucket.primary.count { it.power == PowerState.ON } }
            val total = groups.sumOf { bucket -> bucket.primary.size }
            val summary = if (statesOk) texts.onSummary(on, total) else null
            items += if (room.isBlank()) {
                ControlItem.Header(SECTION_OTHER, texts.otherDevices, summary)
            } else {
                ControlItem.Header(SECTION_ROOM_PREFIX + room, room, summary)
            }
            groups.forEach { bucket -> items += bucket.toItem(pending, statesOk, expandedDevices) }
        }
        if (dead.isNotEmpty()) {
            items += ControlItem.Header(SECTION_UNAVAILABLE, texts.unavailable, dead.size.toString())
            dead.mapTo(items) { ControlItem.Device(it.copy(title = it.displayTitle()), it.id in pending, statesOk) }
        }
        return items
    }

    /**
     * Groups one room's entries by device. An entry without a [ControlEntry.deviceId]
     * is its own device, so an older server keeps rendering as before.
     */
    private fun groupByDevice(entries: List<ControlEntry>): List<DeviceBucket> {
        val buckets = LinkedHashMap<String, MutableList<ControlEntry>>()
        entries.forEach { entry ->
            val key = entry.deviceId.ifBlank { entry.id }
            buckets.getOrPut(key) { mutableListOf() } += entry
        }
        return buckets.map { (deviceId, members) ->
            val primary = members.filter { it.role != ControlRole.SETTING }
            val settings = members.filter { it.role == ControlRole.SETTING }
            val name = members.firstNotNullOfOrNull { it.device.takeIf(String::isNotBlank) }
            DeviceBucket(
                deviceId = deviceId,
                title = name ?: primary.firstOrNull()?.displayTitle() ?: members.first().displayTitle(),
                primary = primary,
                settings = settings,
            )
        }
    }

    private data class DeviceBucket(
        val deviceId: String,
        val title: String,
        val primary: List<ControlEntry>,
        val settings: List<ControlEntry>,
    ) {
        fun toItem(pending: Set<String>, statesOk: Boolean, expandedDevices: Set<String>): ControlItem =
            // A lone primary control is still an ordinary device row; the device
            // name only replaces the title when the entity has none of its own.
            if (primary.size == 1 && settings.isEmpty()) {
                val entry = primary.single()
                ControlItem.Device(entry.copy(title = entry.displayTitle()), entry.id in pending, statesOk)
            } else {
                ControlItem.DeviceGroup(
                    deviceId = deviceId,
                    title = title,
                    // A lone primary is the card's main row: its title is the
                    // device name (the card has no separate header above it).
                    primary = primary.map {
                        val entry = if (primary.size == 1) it.copy(title = title) else it
                        ControlItem.DeviceGroup.Row(entry, it.id in pending)
                    },
                    settings = settings.map { ControlItem.DeviceGroup.Row(it, it.id in pending) },
                    actions = emptyList(),
                    expanded = deviceId in expandedDevices,
                    statesKnown = statesOk,
                )
            }
    }
}

/**
 * Title of a standalone device row: the device name when it is known (the
 * server's entity title is a short label like "Light"), otherwise the entity
 * title. A device with several entries shows the short titles on its rows.
 */
internal fun ControlEntry.displayTitle(): String = device.ifBlank { title }
