package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CompactControlsTest {
    private fun entry(
        kind: ControlKind,
        actions: Set<ControlAction> = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        light: LightControls? = null,
        volume: VolumeControls? = null,
        role: ControlRole = ControlRole.PRIMARY,
    ) = ControlEntry(
        id = "entry.${kind.name.lowercase()}",
        kind = kind,
        title = "Control",
        room = "Bedroom",
        icon = "",
        actions = actions,
        power = PowerState.OFF,
        available = true,
        light = light,
        volume = volume,
        role = role,
    )

    @Test
    fun aPlainSwitchInAMultiControlCardIsATile() {
        assertTrue(entry(ControlKind.SWITCH).isCompactTile(deviceControls = 2))
    }

    @Test
    fun anAdjustableLightStaysARow() {
        val light = LightControls(brightness = true, colorTemp = false, color = false)
        assertFalse(entry(ControlKind.LIGHT, light = light).isCompactTile(deviceControls = 2))
    }

    @Test
    fun aPlainLightWithoutAdjustmentsIsATile() {
        assertTrue(entry(ControlKind.LIGHT).isCompactTile(deviceControls = 2))
    }

    @Test
    fun aVolumeControlStaysARow() {
        val volume = entry(
            ControlKind.VOLUME,
            actions = setOf(ControlAction.SET, ControlAction.MUTE, ControlAction.UNMUTE),
            volume = VolumeControls(level = 45),
        )
        assertFalse(volume.isCompactTile(deviceControls = 2))
    }

    @Test
    fun aButtonOrActionStaysARow() {
        val button = entry(ControlKind.BUTTON, actions = setOf(ControlAction.PRESS))
        val action = entry(ControlKind.ACTION, actions = setOf(ControlAction.SPEAK))
        assertFalse(button.isCompactTile(deviceControls = 2))
        assertFalse(action.isCompactTile(deviceControls = 2))
    }

    @Test
    fun aControlWithExtraActionsStaysARow() {
        val extra = entry(
            ControlKind.SWITCH,
            actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF, ControlAction.SET),
        )
        assertFalse(extra.isCompactTile(deviceControls = 2))
    }

    @Test
    fun aSettingStaysARow() {
        val setting = entry(ControlKind.SWITCH, role = ControlRole.SETTING)
        assertFalse(setting.isCompactTile(deviceControls = 2))
    }

    @Test
    fun aSingleControlCardStaysARow() {
        assertFalse(entry(ControlKind.SWITCH).isCompactTile(deviceControls = 1))
    }

    @Test
    fun anUnknownBinaryKindIsATileWithoutAnyKindList() {
        val unknown = entry(ControlKind.OTHER)
        assertTrue(unknown.isCompactTile(deviceControls = 2))
    }
}
