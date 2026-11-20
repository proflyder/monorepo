package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry

fun ControlEntry.isCompactTile(deviceControls: Int, isMain: Boolean = false): Boolean {
    if (deviceControls < 2 || isMain) return false
    if (isVolume || volume != null) return false
    if (isButton || isProfile) return false
    return ControlAction.TURN_ON in actions && ControlAction.TURN_OFF in actions
}
