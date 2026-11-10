package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import org.junit.Assert.assertEquals
import org.junit.Test

class SpeedDialTest {
    private fun entry(id: String, kind: ControlKind = ControlKind.SWITCH) = ControlEntry(
        id = id,
        kind = kind,
        title = id,
        room = "",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.OFF,
        available = true,
    )

    @Test
    fun dialListsTilesInOrderThenWakeMarks() {
        val tiles = listOf(QuickTile.Call, QuickTile.Device(entry("switch.lamp"), busy = false))
        val wake = listOf(
            WakeFeedbackAction(
                kind = WakeFeedbackKind.FALSE_ALARM,
                entry = entry(WakeFeedback.FALSE_ALARM_ENTRY_ID, ControlKind.SCENARIO),
                busy = false,
            ),
        )
        val items = SpeedDialItems.build(tiles, wake)
        assertEquals(
            listOf("tile:call", "tile:device:switch.lamp", "wake:FALSE_ALARM"),
            items.map { it.key },
        )
    }

    @Test
    fun aNewPinnedDeviceBecomesADialItemWithoutExtraMapping() {
        val tiles = listOf(QuickTile.Device(entry("switch.new"), busy = false))
        assertEquals(
            listOf("tile:device:switch.new"),
            SpeedDialItems.build(tiles, emptyList()).map { it.key },
        )
    }
}
