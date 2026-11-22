package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.PowerState

internal sealed interface ControlItem {
    val key: String

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

    data class DeviceGroup(
        val deviceId: String,
        val title: String,
        val primary: List<Row>,
        val settings: List<Row>,
        val actions: List<Row> = emptyList(),
        val expanded: Boolean,
        val statesKnown: Boolean,
        val mainId: String? = null,
    ) : ControlItem {
        override val key: String get() = "device:$deviceId"

        data class Row(val entry: ControlEntry, val busy: Boolean)
    }

    data class Hub(
        val cards: List<HubCard>,
        val remaining: Map<String, Long>,
        val nowUnix: Long,
    ) : ControlItem {
        override val key: String get() = "hub"
    }

    data class Filters(val chips: List<FilterChip>, val selected: ControlFilter) : ControlItem {
        override val key: String get() = "filters"
    }

    data class FilterChip(val filter: ControlFilter, val label: String, val count: Int)
}

internal data class ControlFilterOption(val filter: ControlFilter, val count: Int)

internal fun ControlEntry.isVisible(entries: List<ControlEntry>): Boolean {
    val condition = visibleWhen ?: return true
    val target = entries.firstOrNull { it.id == condition.entry } ?: return false
    val on = when (target.power) {
        PowerState.ON -> true
        PowerState.OFF -> false
        null -> return false
    }
    return when (condition.state.trim().lowercase()) {
        "on" -> on
        "off" -> !on
        else -> false
    }
}

internal fun visibleEntries(entries: List<ControlEntry>): List<ControlEntry> =
    entries.filter { it.isVisible(entries) }

internal class ControlTexts(
    val otherDevices: String,
    val scenarios: String,
    val unavailable: String,
    val onSummary: (on: Int, total: Int) -> String,
    val all: String,
    val lights: String,
    val switches: String,
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

internal object ControlItems {
    private const val SECTION_UNAVAILABLE = "unavailable"
    private const val SECTION_OTHER = "other"
    private const val SECTION_ROOM_PREFIX = "room:"

    private fun ControlEntry.matches(filter: ControlFilter): Boolean = when (filter) {
        ControlFilter.All -> true
        ControlFilter.Scenarios -> false
        ControlFilter.Lights -> kind == ControlKind.LIGHT
        ControlFilter.Switches -> kind == ControlKind.SWITCH
        is ControlFilter.Room -> room == filter.name
    }

    private fun ControlEntry.isCounted(): Boolean = (isDevice || isVolume) && role != ControlRole.SETTING

    fun options(entries: List<ControlEntry>): List<ControlFilterOption> {
        if (entries.isEmpty()) return emptyList()
        val visible = visibleEntries(entries)
        val counted = visible.filter { it.isCounted() }
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

    fun effectiveFilter(entries: List<ControlEntry>, filter: ControlFilter): ControlFilter =
        if (entries.isEmpty() || options(entries).any { it.filter == filter }) filter else ControlFilter.All

    fun build(
        entries: List<ControlEntry>,
        pending: Set<String>,
        statesOk: Boolean,
        filter: ControlFilter,
        texts: ControlTexts,
        expandedDevices: Set<String> = emptySet(),
        leading: List<ControlItem> = emptyList(),
        collapsedRooms: Set<String> = emptySet(),
    ): List<ControlItem> {
        val visible = visibleEntries(entries)
        val applied = effectiveFilter(visible, filter)
        val content = visible.filterNot { it.isProfile }
        val items = mutableListOf<ControlItem>()
        items += leading

        val filterOptions = options(visible)
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

    private fun prefixOf(entry: ControlEntry): String = localId(entry).substringBefore('_')

    private fun prefixGroups(entries: List<ControlEntry>): Set<Pair<String, String>> =
        entries.filter { it.deviceId.isBlank() }
            .groupBy { it.room to prefixOf(it) }
            .filter { (_, members) ->
                members.size > 1 && members.any { localId(it) == prefixOf(it) }
            }
            .keys

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

    private fun mergeVolumes(members: List<ControlEntry>): List<ControlEntry> {
        val volumes = members.filter { it.isVolume }
        if (volumes.size < 2) return members
        val main = volumes.firstOrNull { ControlAction.MUTE in it.actions } ?: volumes.first()
        val companions = volumes.filter { it.id != main.id }
        val merged = main.copy(volumeCompanion = companions.firstOrNull())
        val companionIds = companions.map { it.id }.toSet()
        return members.map { if (it.id == main.id) merged else it }.filter { it.id !in companionIds }
    }

    private fun ControlEntry.namesDevice(deviceName: String?): Boolean {
        val target = deviceName?.trim().orEmpty()
        if (target.isEmpty()) return false
        if (title.trim().equals(target, ignoreCase = true)) return true
        return isDevice && localId(this).substringAfterLast('_').equals("power", ignoreCase = true)
    }

    private fun bucketOf(deviceId: String, members: List<ControlEntry>, prefix: String? = null): DeviceBucket {
        val entries = mergeVolumes(members)
        val anchor = prefix?.let { value -> entries.firstOrNull { localId(it) == value } }
        val hasPrimary = entries.any { it.role != ControlRole.SETTING && !it.isButton }
        val settings = entries.filter { it.role == ControlRole.SETTING || (it.isButton && hasPrimary) }
        val primary = entries.filter { entry -> settings.none { it.id == entry.id } }
        val name = entries.firstNotNullOfOrNull { it.device.takeIf(String::isNotBlank) }
            ?: anchor?.displayTitle()
        val main = anchor
            ?: primary.firstOrNull { it.namesDevice(name) }
            ?: primary.singleOrNull()
        return DeviceBucket(
            deviceId = deviceId,
            title = name ?: primary.firstOrNull()?.displayTitle() ?: entries.first().displayTitle(),
            primary = primary,
            settings = settings,
            mainId = main?.id,
        )
    }

    private data class DeviceBucket(
        val deviceId: String,
        val title: String,
        val primary: List<ControlEntry>,
        val settings: List<ControlEntry>,
        val mainId: String?,
    ) {
        fun toItem(pending: Set<String>, statesOk: Boolean, expandedDevices: Set<String>): ControlItem =
            if (primary.size == 1 && settings.isEmpty()) {
                val entry = primary.single()
                ControlItem.Device(entry.copy(title = entry.displayTitle()), entry.id in pending, statesOk)
            } else {
                ControlItem.DeviceGroup(
                    deviceId = deviceId,
                    title = title,
                    primary = primary.map {
                        val entry = if (primary.size == 1) it.copy(title = title) else it
                        ControlItem.DeviceGroup.Row(entry, it.id in pending)
                    },
                    settings = settings.map { ControlItem.DeviceGroup.Row(it, it.id in pending) },
                    actions = emptyList(),
                    expanded = deviceId in expandedDevices,
                    statesKnown = statesOk,
                    mainId = mainId,
                )
            }
    }
}

internal fun ControlEntry.displayTitle(): String = displayName
