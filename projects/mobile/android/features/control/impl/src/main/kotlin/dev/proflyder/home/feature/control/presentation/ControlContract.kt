package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.feature.control.ControlCatalogState
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.QuickTiles
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.SpeedDialItems
import dev.proflyder.home.feature.control.WakeFeedback
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.WakeFeedbackKind

internal data class ControlLayout(
    val expandedDevices: Set<String> = emptySet(),
    val collapsedRooms: Set<String> = emptySet(),
    val dialOpen: Boolean = false,
)

internal data class HubView(
    val cards: List<HubCard> = emptyList(),
    val remaining: Map<String, Long> = emptyMap(),
    val nowUnix: Long = 0,
)

internal data class ControlUiState(
    val catalog: ControlCatalogState = ControlCatalogState(),
    val hub: HubView = HubView(),
    val scenarios: List<Scenario> = emptyList(),
    val favorites: List<FavoriteRef> = emptyList(),
    val provided: List<ProvidedTile> = emptyList(),
    val filter: ControlFilter = ControlFilter.All,
    val pending: Set<String> = emptySet(),
    val pendingScenarios: Set<String> = emptySet(),
    val layout: ControlLayout = ControlLayout(),
) {
    val entries: List<ControlEntry> get() = catalog.entries

    val speedDial: List<SpeedDialItem>
        get() = SpeedDialItems.build(
            QuickTiles.build(provided, favorites, entries, scenarios, pending, pendingScenarios),
            WakeFeedback.actions(entries, pending),
        )
}

internal sealed interface PinSubject {
    val ref: FavoriteRef

    data class Entry(val entry: ControlEntry, override val ref: FavoriteRef) : PinSubject

    data class ScenarioPin(val scenario: Scenario, override val ref: FavoriteRef) : PinSubject
}

internal sealed interface TapTarget {
    data class Item(val item: ControlItem) : TapTarget

    data class Tile(val tile: QuickTile) : TapTarget

    data class Dial(val item: SpeedDialItem) : TapTarget

    data class Hub(val card: HubCard) : TapTarget
}

internal sealed interface DeviceCommand {
    data class Run(val entry: ControlEntry) : DeviceCommand

    data class Wake(val action: WakeFeedbackAction) : DeviceCommand

    data class Press(val entry: ControlEntry) : DeviceCommand

    data class Speak(val entry: ControlEntry, val text: String) : DeviceCommand

    data class Power(val entry: ControlEntry, val on: Boolean) : DeviceCommand

    data class Adjust(val entry: ControlEntry, val adjust: LightAdjust) : DeviceCommand

    data class Volume(val entry: ControlEntry, val level: Int) : DeviceCommand

    data class Mute(val entry: ControlEntry, val muted: Boolean) : DeviceCommand

    data class RunScenario(val scenario: Scenario) : DeviceCommand
}

internal sealed interface HubCommand {
    data class Primary(val card: HubCard) : HubCommand

    data class Switch(val card: HubCard.Alarm, val enabled: Boolean) : HubCommand

    data class Disable(val card: HubCard.Alarm) : HubCommand

    data class Skip(val card: HubCard.Alarm) : HubCommand
}

internal sealed interface ControlIntent {
    data object Refresh : ControlIntent

    data class Tap(val target: TapTarget) : ControlIntent

    data class Hold(val target: TapTarget) : ControlIntent

    data class Command(val command: DeviceCommand) : ControlIntent

    data class Hub(val command: HubCommand) : ControlIntent

    data class AskPin(val entry: ControlEntry) : ControlIntent

    data class TogglePin(val subject: PinSubject) : ControlIntent

    data class SelectFilter(val filter: ControlFilter) : ControlIntent

    data class ToggleDevice(val deviceId: String) : ControlIntent

    data class ToggleRoom(val section: String) : ControlIntent

    data class Dial(val open: Boolean) : ControlIntent

    data class ActivateTile(val key: String) : ControlIntent
}

internal sealed interface ControlNotice {
    data class ScenarioQueued(val title: String) : ControlNotice

    data class FavoriteChanged(val subject: PinSubject, val pinned: Boolean) : ControlNotice

    data class WakeMarked(val kind: WakeFeedbackKind) : ControlNotice

    data object CommandSent : ControlNotice
}

internal sealed interface ControlEvent {
    data class Notice(val notice: ControlNotice) : ControlEvent

    data class Failure(val failure: RemoteFailure) : ControlEvent

    data class OpenLight(val entry: ControlEntry) : ControlEvent

    data class OpenVolume(val entry: ControlEntry, val announce: ControlEntry?) : ControlEvent

    data class OpenSpeak(val entry: ControlEntry) : ControlEvent

    data class ConfirmPin(val entry: ControlEntry, val pinned: Boolean) : ControlEvent

    data class AskAlarmDisable(val card: HubCard.Alarm) : ControlEvent

    data class OpenFeature(val featureId: String) : ControlEvent
}
