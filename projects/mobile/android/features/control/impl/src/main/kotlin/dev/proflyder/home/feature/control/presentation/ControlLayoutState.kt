package dev.proflyder.home.feature.control.presentation

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

internal class ControlLayoutState {
    private val layout = MutableStateFlow(ControlLayout())

    val state: StateFlow<ControlLayout> = layout.asStateFlow()

    fun toggleDevice(deviceId: String) = layout.update {
        it.copy(expandedDevices = it.expandedDevices.toggle(deviceId))
    }

    fun toggleRoom(section: String) = layout.update {
        it.copy(collapsedRooms = it.collapsedRooms.toggle(section))
    }

    fun setDial(open: Boolean) = layout.update { it.copy(dialOpen = open) }

    private fun Set<String>.toggle(value: String): Set<String> = if (value in this) this - value else this + value
}
