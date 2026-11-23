package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.SpeedDialItem

internal sealed interface ControlReaction {
    data class Dispatch(val intent: ControlIntent) : ControlReaction

    data class Emit(val event: ControlEvent) : ControlReaction

    data object None : ControlReaction
}

internal object ControlTaps {
    fun tap(target: TapTarget, state: ControlUiState): ControlReaction = when (target) {
        is TapTarget.Item -> tapItem(target.item, state)
        is TapTarget.Tile -> tapTile(target.tile, state)
        is TapTarget.Dial -> tapDial(target.item)
        is TapTarget.Hub -> ControlReaction.Emit(ControlEvent.OpenRoute(Route.Tab(target.card.featureId)))
    }

    fun hold(target: TapTarget, state: ControlUiState): ControlReaction = when (target) {
        is TapTarget.Item -> holdItem(target.item, state)
        is TapTarget.Tile -> holdTile(target.tile, state)
        is TapTarget.Dial -> holdDial(target.item)
        is TapTarget.Hub -> ControlReaction.None
    }

    private fun tapItem(item: ControlItem, state: ControlUiState): ControlReaction = when (item) {
        is ControlItem.Scenario -> when (item.entry.kind) {
            ControlKind.ACTION -> ControlReaction.Emit(ControlEvent.OpenSpeak(item.entry))
            else -> command(DeviceCommand.Run(item.entry))
        }
        is ControlItem.Device -> tapDevice(item.entry, state)
        is ControlItem.Header, is ControlItem.DeviceGroup, is ControlItem.Hub, is ControlItem.Filters ->
            ControlReaction.None
    }

    private fun tapTile(tile: QuickTile, state: ControlUiState): ControlReaction = when (tile) {
        is QuickTile.Device -> tapDevice(tile.entry, state)
        is QuickTile.Speak -> ControlReaction.Emit(ControlEvent.OpenSpeak(tile.entry))
        is QuickTile.ScenarioTile -> command(DeviceCommand.RunScenario(tile.scenario))
        is QuickTile.Provided -> ControlReaction.Dispatch(ControlIntent.ActivateTile(tile.tile.key))
    }

    private fun tapDial(item: SpeedDialItem): ControlReaction = when (item) {
        is SpeedDialItem.Wake -> command(DeviceCommand.Wake(item.action))
        is SpeedDialItem.Tile -> when (val tile = item.tile) {
            is QuickTile.Device -> command(DeviceCommand.Power(tile.entry, !tile.entry.isOn))
            else -> tapTile(tile, ControlUiState())
        }
    }

    private fun tapDevice(entry: ControlEntry, state: ControlUiState): ControlReaction =
        sheetFor(entry, state)?.let(ControlReaction::Emit)
            ?: command(DeviceCommand.Power(entry, !entry.isOn))

    private fun holdItem(item: ControlItem, state: ControlUiState): ControlReaction {
        val entry = (item as? ControlItem.Device)?.entry ?: return ControlReaction.None
        return when {
            entry.light != null && entry.available -> ControlReaction.Emit(ControlEvent.OpenLight(entry))
            else -> ControlReaction.Emit(ControlEvent.ConfirmPin(entry, entry.devicePin in state.favorites))
        }
    }

    private fun holdTile(tile: QuickTile, state: ControlUiState): ControlReaction = when (tile) {
        is QuickTile.Device -> sheetFor(tile.entry.copy(available = true), state)?.let(ControlReaction::Emit)
            ?: pin(tile)
        else -> pin(tile)
    }

    private fun holdDial(item: SpeedDialItem): ControlReaction = when (item) {
        is SpeedDialItem.Tile -> pin(item.tile)
        is SpeedDialItem.Wake -> ControlReaction.None
    }

    private fun pin(tile: QuickTile): ControlReaction = tile.pinSubject()
        ?.let { ControlReaction.Dispatch(ControlIntent.TogglePin(it)) }
        ?: ControlReaction.None

    private fun sheetFor(entry: ControlEntry, state: ControlUiState): ControlEvent? = when {
        !entry.available -> null
        entry.isVolume -> ControlEvent.OpenVolume(entry, entry.announceIn(state.entries))
        entry.light != null -> ControlEvent.OpenLight(entry)
        else -> null
    }

    private fun command(command: DeviceCommand): ControlReaction =
        ControlReaction.Dispatch(ControlIntent.Command(command))
}

internal val ControlEntry.isOn: Boolean get() = power == PowerState.ON

internal val ControlEntry.devicePin: FavoriteRef get() = FavoriteRef(FavoriteKind.DEVICE, id)

internal fun ControlEntry.announceIn(entries: List<ControlEntry>): ControlEntry? =
    volumeCompanion ?: entries.firstOrNull { it.isVolume && it.deviceId == deviceId && it.id != id }

internal fun QuickTile.pinSubject(): PinSubject? = when (this) {
    is QuickTile.Device -> PinSubject.Entry(entry, FavoriteRef(FavoriteKind.DEVICE, entry.id))
    is QuickTile.Speak -> PinSubject.Entry(entry, FavoriteRef(FavoriteKind.ACTION, entry.id))
    is QuickTile.ScenarioTile -> PinSubject.ScenarioPin(scenario, FavoriteRef(FavoriteKind.SCENARIO, scenario.id))
    is QuickTile.Provided -> null
}

internal val HubCard.featureId: String
    get() = when (this) {
        is HubCard.TimerCard -> "timers"
        is HubCard.Reminder -> "reminders"
    }
