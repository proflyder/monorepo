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
     * room that happens to be called "Home" never collides with another
     * section. [summary] is a short right-aligned note ("2 of 4 on").
     * Room headers are [collapsible]: tapping them hides or shows the devices.
     */
    data class Header(
        val section: String,
        val title: String,
        val summary: String? = null,
        val collapsible: Boolean = false,
        val collapsed: Boolean = false,
    ) : ControlItem {
        override val key: String get() = "header:$section"
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

    /** The compact live-rows block; [remaining] holds server-accurate timer seconds for this tick. */
    data class Hub(
        val cards: List<HubCard>,
        val remaining: Map<String, Long>,
        val nowUnix: Long,
    ) : ControlItem {
        override val key: String get() = "hub"
    }

    /** The pinned "Quick actions" tiles; [hint] shows the empty-state hint. */
    data class QuickActions(val tiles: List<QuickTile>, val hint: Boolean) : ControlItem {
        override val key: String get() = "quick_actions"
    }

    /**
     * The single compact "Voice" card: the assistant profile (a global mode,
     * not a device) and the one-tap marks of a live wake event. Either part can
     * be absent; the card itself exists only while one of them is.
     */
    data class Voice(
        val profile: Profile? = null,
        val actions: List<WakeFeedbackAction> = emptyList(),
    ) : ControlItem {
        override val key: String get() = "voice"

        /** The profile is a mode: only the segmented pills act, never a row tap. */
        data class Profile(val entry: ControlEntry, val busy: Boolean)
    }

    /** Device filter chips, placed directly above the room sections they filter. */
    data class Filters(val chips: List<FilterChip>, val selected: ControlFilter) : ControlItem {
        override val key: String get() = "filters"
    }

    data class FilterChip(val filter: ControlFilter, val label: String, val count: Int)
}

/** A selectable chip: the filter plus how many entries it shows. */
data class ControlFilterOption(val filter: ControlFilter, val count: Int)

/** Texts the list needs; the presentation layer supplies localized strings. */
class ControlTexts(
    val otherDevices: String,
    val scenarios: String,
    val unavailable: String,
    /** Section title of the combined voice card. */
    val voice: String,
    val onSummary: (on: Int, total: Int) -> String,
    /** Header of the device-level action cards ("Say on the station"). */
    val actions: String = "Actions",
    val all: String = "All",
    val lights: String = "Lights",
    val switches: String = "Switches",
) {
    fun filterLabel(filter: ControlFilter, count: Int): String {
        val name = when (filter) {
            ControlFilter.All -> all
            ControlFilter.Scenarios -> scenarios
            ControlFilter.Lights -> lights
            ControlFilter.Switches -> switches
            is ControlFilter.Room -> filter.name
        }
        return "$name · $count"
    }
}

object ControlItems {
    private const val SECTION_VOICE = "voice"
    private const val SECTION_UNAVAILABLE = "unavailable"
    private const val SECTION_OTHER = "other"
    private const val SECTION_ROOM_PREFIX = "room:"

    private fun ControlEntry.matches(filter: ControlFilter): Boolean = when (filter) {
        ControlFilter.All -> true
        // HA scripts are gone from Home; the native scenarios live in their own tab.
        ControlFilter.Scenarios -> false
        ControlFilter.Lights -> kind == ControlKind.LIGHT
        ControlFilter.Switches -> kind == ControlKind.SWITCH
        is ControlFilter.Room -> room == filter.name
    }

    /**
     * The chip counts describe primary controls only: a device's settings are
     * part of it and never add a chip or a count of their own.
     */
    private fun ControlEntry.isCounted(): Boolean = (isDevice || isVolume) && role != ControlRole.SETTING

    /**
     * The chips: All, then the device categories that exist, then each room
     * (only when there is more than one, otherwise rooms add nothing). The
     * profile selector is pinned and always shown, so it is never a chip and
     * never counted: the counts describe filterable content only. Native
     * scenarios are not part of Home and have no chip.
     */
    fun options(entries: List<ControlEntry>): List<ControlFilterOption> {
        if (entries.isEmpty()) return emptyList()
        val counted = entries.filter { it.isCounted() }
        val options = mutableListOf(ControlFilterOption(ControlFilter.All, counted.size))
        listOf(ControlFilter.Lights, ControlFilter.Switches).forEach { filter ->
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
     * [leading] items (the live rows and the pinned quick actions) come
     * first, then the single voice card (a mode and wake marks, not content,
     * shown whatever the filter is), then available devices grouped by room
     * and, inside a room, by device, then unavailable devices in their own
     * trailing section so dead entries do not interleave with working ones.
     *
     * [expandedDevices] holds the device ids whose "Settings (N)" row is open;
     * [collapsedRooms] holds section ids of collapsed room groups;
     * [wakeFeedback] holds the resolved wake-mark actions for the voice card.
     */
    fun build(
        entries: List<ControlEntry>,
        pending: Set<String>,
        statesOk: Boolean,
        filter: ControlFilter,
        texts: ControlTexts,
        expandedDevices: Set<String> = emptySet(),
        leading: List<ControlItem> = emptyList(),
        collapsedRooms: Set<String> = emptySet(),
        wakeFeedback: List<WakeFeedbackAction> = emptyList(),
    ): List<ControlItem> {
        val applied = effectiveFilter(entries, filter)
        val profile = entries.firstOrNull { it.isProfile }
        val content = entries.filterNot { it.isProfile }
        val items = mutableListOf<ControlItem>()
        items += leading

        // One card for the whole voice block: profile and/or wake marks.
        if (profile != null || wakeFeedback.isNotEmpty()) {
            items += ControlItem.Header(SECTION_VOICE, texts.voice)
            items += ControlItem.Voice(
                profile = profile?.let { ControlItem.Voice.Profile(it, busy = it.id in pending) },
                actions = wakeFeedback,
            )
        }

        // Chips filter devices only, so they sit directly above the rooms
        // instead of floating over the live rows and quick actions.
        val filterOptions = options(entries)
        if (filterOptions.size > 1) {
            items += ControlItem.Filters(
                chips = filterOptions.map { option ->
                    ControlItem.FilterChip(
                        filter = option.filter,
                        label = texts.filterLabel(option.filter, option.count),
                        count = option.count,
                    )
                },
                selected = applied,
            )
        }

        val devices = content.filter { it.isDevice || it.isButton || it.isVolume }
        val (working, deadEntries) = devices.partition { it.available }
        // Dead rows keep the old behaviour: they follow the applied filter too,
        // and a lone setting never shows up as its own unavailable device.
        val dead = deadEntries.filter { applied == ControlFilter.All || (it.role != ControlRole.SETTING && it.matches(applied)) }
        val byRoom = LinkedHashMap<String, MutableList<ControlEntry>>()
        working.forEach { byRoom.getOrPut(it.room) { mutableListOf() } += it }
        byRoom.forEach { (room, roomEntries) ->
            items += roomSection(
                room = room,
                roomEntries = roomEntries,
                applied = applied,
                statesOk = statesOk,
                texts = texts,
                pending = pending,
                expandedDevices = expandedDevices,
                collapsedRooms = collapsedRooms,
            )
        }
        if (dead.isNotEmpty()) {
            items += ControlItem.Header(SECTION_UNAVAILABLE, texts.unavailable, dead.size.toString())
            dead.mapTo(items) { ControlItem.Device(it.copy(title = it.displayTitle()), it.id in pending, statesOk) }
        }
        return items
    }

    private fun roomSection(
        room: String,
        roomEntries: List<ControlEntry>,
        applied: ControlFilter,
        statesOk: Boolean,
        texts: ControlTexts,
        pending: Set<String>,
        expandedDevices: Set<String>,
        collapsedRooms: Set<String>,
    ): List<ControlItem> {
        // A group is shown when one of its primary controls matches the
        // filter; its settings then follow the device, never the chip.
        val groups = groupByDevice(roomEntries).filter { it.matchesFilter(applied) }
        if (groups.isEmpty()) return emptyList()
        val on = groups.sumOf { bucket -> bucket.primary.count { it.power == PowerState.ON } }
        val total = groups.sumOf { bucket -> bucket.primary.size }
        val summary = if (statesOk) texts.onSummary(on, total) else null
        val section = if (room.isBlank()) SECTION_OTHER else SECTION_ROOM_PREFIX + room
        val collapsed = section in collapsedRooms
        val result = mutableListOf<ControlItem>()
        result += if (room.isBlank()) {
            ControlItem.Header(section, texts.otherDevices, summary, collapsible = true, collapsed = collapsed)
        } else {
            ControlItem.Header(section, room, summary, collapsible = true, collapsed = collapsed)
        }
        if (!collapsed) {
            groups.forEach { bucket -> result += bucket.toItem(pending, statesOk, expandedDevices) }
        }
        return result
    }

    private fun DeviceBucket.matchesFilter(filter: ControlFilter): Boolean =
        filter == ControlFilter.All || primary.any { it.matches(filter) }

    private fun localId(entry: ControlEntry): String = entry.id.substringAfter('.', entry.id)

    /** The part of the entity id before the first `_`, e.g. `komp` in `button.komp_lock`. */
    private fun prefixOf(entry: ControlEntry): String = localId(entry).substringBefore('_')

    /**
     * Prefix groups present in [entries]: entities without a registry device id
     * that share a room and the entity-id prefix of a base entity (`switch.komp`
     * is the base of `button.komp_lock`/`button.komp_unlock`). Home Assistant
     * template entities have no device registry entry, so this is how their
     * buttons still join the device card.
     */
    private fun prefixGroups(entries: List<ControlEntry>): Set<Pair<String, String>> =
        entries.filter { it.deviceId.isBlank() }
            .groupBy { it.room to prefixOf(it) }
            .filter { (_, members) ->
                members.size > 1 && members.any { localId(it) == prefixOf(it) }
            }
            .keys

    /**
     * Groups one room's entries by device. An entry with a server device id
     * joins that device; entities without one join a prefix group when their
     * shared prefix has a base entity, and are otherwise each their own device,
     * so an older server keeps rendering as before.
     */
    private fun groupByDevice(entries: List<ControlEntry>): List<DeviceBucket> {
        val prefixes = prefixGroups(entries)
        val buckets = LinkedHashMap<String, MutableList<ControlEntry>>()
        entries.forEach { entry ->
            val group = entry.room to prefixOf(entry)
            val key = when {
                entry.deviceId.isNotBlank() -> "device:${entry.deviceId}"
                group in prefixes -> "prefix:${group.first}:${group.second}"
                else -> "entry:${entry.id}"
            }
            buckets.getOrPut(key) { mutableListOf() } += entry
        }
        return buckets.map { (key, members) ->
            val prefix = prefixOf(members.first()).takeIf { key.startsWith("prefix:") }
            val deviceId = if (prefix != null) key else members.first().deviceId.ifBlank { key }
            bucketOf(deviceId, members, prefix)
        }
    }

    private fun bucketOf(deviceId: String, members: List<ControlEntry>, prefix: String? = null): DeviceBucket {
        // A base entity names a prefix group; it may not sort first (the catalog
        // is ordered by title), so look it up explicitly.
        val anchor = prefix?.let { value -> members.firstOrNull { localId(it) == value } }
        val hasPrimary = members.any { it.role != ControlRole.SETTING && !it.isButton }
        // Buttons are one-shot device actions: they live in the settings list
        // next to the device's other options when that device has a primary row.
        val settings = members.filter { it.role == ControlRole.SETTING || (it.isButton && hasPrimary) }
        val primary = members.filter { entry -> settings.none { it.id == entry.id } }
        val name = members.firstNotNullOfOrNull { it.device.takeIf(String::isNotBlank) }
            ?: anchor?.displayTitle()
        return DeviceBucket(
            deviceId = deviceId,
            title = name ?: primary.firstOrNull()?.displayTitle() ?: members.first().displayTitle(),
            primary = primary,
            settings = settings,
        )
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
 * Title of a standalone device row: the human name, preferring the device name
 * when the server only sent a technical entity title ("Light", "Switch").
 */
internal fun ControlEntry.displayTitle(): String = displayName
