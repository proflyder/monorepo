package dev.proflyder.home.core.model

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

data class CatalogDeviceGroup(val title: String, val entries: List<ControlEntry>)

data class CatalogRoomGroup(val room: String, val devices: List<CatalogDeviceGroup>)

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
                    entries = members.sortedBy { it.role == ControlRole.SETTING },
                )
            }
            CatalogRoomGroup(room, devices)
        }
    }

    fun entryLabel(entry: ControlEntry, labels: CatalogGroupingLabels): String {
        val name = entry.displayName.trim()
        val technical = entry.device.isBlank() && entry.isTechnicalEntityTitle()
        return if (name.isEmpty() || technical) labels.kind(entry.kind) else name
    }

    fun needsDeviceHeader(group: CatalogDeviceGroup, labels: CatalogGroupingLabels): Boolean =
        group.entries.size != 1 || entryLabel(group.entries.single(), labels) != group.title

    private fun deviceKey(entry: ControlEntry): String {
        val device = entry.device.trim()
        if (device.isNotEmpty()) return "device:$device"
        val title = entry.title.trim()
        if (title.isNotEmpty() && !entry.isTechnicalEntityTitle()) return "title:$title"
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
