package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind

/** Rows of the Home screen: section headers, scenario tiles and device rows. */
internal sealed interface ControlItem {
    /** Stable identity for DiffUtil. */
    val key: String

    data class Header(val title: String) : ControlItem {
        override val key: String get() = "header:$title"
    }

    data class Scenario(val entry: ControlEntry, val busy: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }

    data class Device(val entry: ControlEntry, val busy: Boolean, val statesKnown: Boolean) : ControlItem {
        override val key: String get() = "entry:${entry.id}"
    }
}

internal object ControlItems {
    /**
     * Scenarios first (one section), then devices grouped by room in
     * first-appearance order; devices without a room go under [devicesTitle].
     */
    fun build(
        state: ControlUiState,
        scenariosTitle: String,
        devicesTitle: String,
    ): List<ControlItem> {
        val items = mutableListOf<ControlItem>()
        val scenarios = state.entries.filter { it.kind == ControlKind.SCENARIO }
        if (scenarios.isNotEmpty()) {
            items += ControlItem.Header(scenariosTitle)
            scenarios.mapTo(items) { ControlItem.Scenario(it, it.id in state.pending) }
        }
        val devices = state.entries.filter { it.isDevice }
        val byRoom = LinkedHashMap<String, MutableList<ControlEntry>>()
        devices.forEach { byRoom.getOrPut(it.room.ifBlank { devicesTitle }) { mutableListOf() } += it }
        byRoom.forEach { (room, entries) ->
            items += ControlItem.Header(room)
            entries.mapTo(items) { ControlItem.Device(it, it.id in state.pending, state.statesOk) }
        }
        return items
    }
}
