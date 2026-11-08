package dev.proflyder.home.core.model

/** Localized fallbacks used when the catalog has no human name for a group or row. */
interface CatalogGroupingLabels {
    fun scenariosHa(): String
    fun other(): String
    fun light(): String
    fun switch(): String
    fun scenario(): String
    fun action(): String
    fun device(): String

    fun kind(kind: ControlKind): String = when (kind) {
        ControlKind.LIGHT -> light()
        ControlKind.SWITCH -> switch()
        ControlKind.SCENARIO -> scenario()
        ControlKind.ACTION -> action()
        else -> device()
    }
}

/** One device-level block of the picker. */
data class CatalogDeviceGroup(val title: String, val entries: List<ControlEntry>)

/** One room-level block of the picker. */
data class CatalogRoomGroup(val room: String, val devices: List<CatalogDeviceGroup>)

/**
 * Pure grouping of the control catalog for the scenario pickers.
 *
 * - The device name is the group title; when a device is missing, a meaningful
 *   entry title is used, otherwise a localized "Scenarios HA"/"Other" kind
 *   fallback. Raw entity ids are never exposed.
 * - Secondary entries (`role = SETTING`, e.g. delay-off helpers, default power
 *   state) are hidden unless explicitly requested.
 */
object CatalogGrouping {
    fun settingsCount(entries: List<ControlEntry>): Int =
        entries.count { it.role == ControlRole.SETTING }

    fun rooms(
        entries: List<ControlEntry>,
        includeSettings: Boolean,
        labels: CatalogGroupingLabels,
    ): List<CatalogRoomGroup> {
        val visible = if (includeSettings) entries else entries.filter { it.role != ControlRole.SETTING }
        return visible.groupBy { it.room }.map { (room, roomEntries) ->
            val devices = roomEntries.groupBy { deviceKey(it) }.map { (_, members) ->
                CatalogDeviceGroup(
                    title = groupTitle(members.first(), labels),
                    // Primary controls first, then the revealed service rows.
                    entries = members.sortedBy { it.role == ControlRole.SETTING },
                )
            }
            CatalogRoomGroup(room, devices)
        }
    }

    /** Human label of one row; technical/id-like titles fall back to the kind name. */
    fun entryLabel(entry: ControlEntry, labels: CatalogGroupingLabels): String {
        val name = entry.displayName.trim()
        val technical = entry.device.isBlank() && entry.isTechnicalEntityTitle()
        return if (name.isEmpty() || technical) labels.kind(entry.kind) else name
    }

    /** A group of one row that repeats its own title needs no separate header. */
    fun needsDeviceHeader(group: CatalogDeviceGroup, labels: CatalogGroupingLabels): Boolean =
        group.entries.size != 1 || entryLabel(group.entries.single(), labels) != group.title

    private fun deviceKey(entry: ControlEntry): String {
        val device = entry.device.trim()
        if (device.isNotEmpty()) return "device:$device"
        val title = entry.title.trim()
        if (title.isNotEmpty() && !entry.isTechnicalEntityTitle()) return "title:$title"
        // Technical entries without a device share one human-named group
        // instead of repeating a fallback header per raw entity.
        return when (entry.kind) {
            ControlKind.SCENARIO -> "group:scenarios"
            else -> "group:other"
        }
    }

    private fun groupTitle(entry: ControlEntry, labels: CatalogGroupingLabels): String {
        val device = entry.device.trim()
        if (device.isNotEmpty()) return device
        val title = entry.title.trim()
        if (title.isNotEmpty() && !entry.isTechnicalEntityTitle()) return title
        return when (entry.kind) {
            ControlKind.SCENARIO -> labels.scenariosHa()
            else -> labels.other()
        }
    }
}
