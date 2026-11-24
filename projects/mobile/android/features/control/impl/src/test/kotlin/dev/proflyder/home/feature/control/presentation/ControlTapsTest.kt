package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import dev.proflyder.home.feature.control.ControlCatalogState
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.core.model.ControlCatalog
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ControlTapsTest {
    private object OpenAccess : dev.proflyder.home.core.access.AppAccessSource {
        override val state = kotlinx.coroutines.flow.MutableStateFlow<dev.proflyder.home.core.access.AppAccessState>(
            dev.proflyder.home.core.access.AppAccessState.Unknown,
        )
    }

    private val switch = ControlEntry(
        id = "switch.fan",
        kind = ControlKind.SWITCH,
        title = "Fan",
        room = "",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.OFF,
        available = true,
    )
    private val light = switch.copy(
        id = "light.desk",
        kind = ControlKind.LIGHT,
        light = LightControls(brightness = true, colorTemp = false, color = false),
    )
    private val volume = switch.copy(id = "volume.station", kind = ControlKind.VOLUME, volume = VolumeControls(level = 10))
    private val state = ControlUiState(
        catalog = ControlCatalogState(catalog = ControlCatalog(listOf(switch, light, volume), statesOk = true)),
    )

    private fun device(entry: ControlEntry) = TapTarget.Item(ControlItem.Device(entry, busy = false, statesKnown = true))

    @Test
    fun tappingASwitchTogglesItThroughACommand() {
        assertEquals(
            ControlReaction.Dispatch(ControlIntent.Command(DeviceCommand.Power(switch, on = true))),
            ControlTaps.tap(device(switch), state),
        )
    }

    @Test
    fun tappingALightOrVolumeOpensItsSheet() {
        assertEquals(ControlReaction.Emit(ControlEvent.OpenLight(light)), ControlTaps.tap(device(light), state))
        assertEquals(ControlReaction.Emit(ControlEvent.OpenVolume(volume, null)), ControlTaps.tap(device(volume), state))
    }

    @Test
    fun anUnavailableLightIsToggledInsteadOfOpened() {
        val dead = light.copy(available = false)
        assertEquals(
            ControlReaction.Dispatch(ControlIntent.Command(DeviceCommand.Power(dead, on = true))),
            ControlTaps.tap(device(dead), state),
        )
    }

    @Test
    fun holdingAPlainDeviceAsksToPinWithTheCurrentPinState() {
        val pinned = state.copy(favorites = listOf(FavoriteRef(FavoriteKind.DEVICE, switch.id)))
        assertEquals(
            ControlReaction.Emit(ControlEvent.ConfirmPin(switch, pinned = true)),
            ControlTaps.hold(device(switch), pinned),
        )
    }

    @Test
    fun aProvidedTileIsActivatedByItsKey() {
        val tile = QuickTile.Provided(ProvidedTile("vpn", order = 1, labelRes = 0, iconRes = 0))
        assertEquals(
            ControlReaction.Dispatch(ControlIntent.ActivateTile("vpn")),
            ControlTaps.tap(TapTarget.Tile(tile), state),
        )
        assertEquals(ControlReaction.None, ControlTaps.hold(TapTarget.Tile(tile), state))
    }

    @Test
    fun aDialDeviceTogglesAndADialHoldPins() {
        val item = SpeedDialItem.Tile(QuickTile.Device(light, busy = false))
        assertEquals(
            ControlReaction.Dispatch(ControlIntent.Command(DeviceCommand.Power(light, on = true))),
            ControlTaps.tap(TapTarget.Dial(item), state),
        )
        assertEquals(
            ControlReaction.Dispatch(ControlIntent.TogglePin(PinSubject.Entry(light, FavoriteRef(FavoriteKind.DEVICE, light.id)))),
            ControlTaps.hold(TapTarget.Dial(item), state),
        )
    }

    @Test
    fun aQuickTileProviderDecidesWhereItsTileLeads() {
        val vpn = object : QuickTileProvider {
            override val tile = ProvidedTile("vpn", order = 1, labelRes = 0, iconRes = 0)

            override fun activate(): QuickTileAction = QuickTileAction.OpenFeature("vpn")
        }
        var calls = 0
        val call = object : QuickTileProvider {
            override val tile = ProvidedTile("call", order = 0, labelRes = 0, iconRes = 0)

            override fun activate(): QuickTileAction = QuickTileAction.Handled.also { calls++ }
        }
        val launcher = QuickTileLauncher(setOf(vpn, call), OpenAccess)
        assertEquals(ControlEvent.OpenRoute(Route.Tab("vpn")), launcher.activate("vpn"))
        assertNull(launcher.activate("call"))
        assertEquals(1, calls)
        assertNull(launcher.activate("missing"))
    }
}
