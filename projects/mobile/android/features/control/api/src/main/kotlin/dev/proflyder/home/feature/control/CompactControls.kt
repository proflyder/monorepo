package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry

/**
 * One pure rule deciding how a control inside a device card is drawn: a round
 * compact tile or a normal row.
 *
 * A tile is a binary on/off control — it accepts both turn_on and turn_off —
 * inside a card that holds at least two controls. The card's main control (the
 * one that names the device and carries its status) always stays a row, as do
 * volume controls, buttons/actions and the global profile. Extra capabilities
 * such as a light's brightness or colour do not turn a binary control into a
 * row: its tap still toggles, and a long press opens its adjustment sheet.
 * There is no list of known kinds, so a new binary kind becomes a tile
 * automatically.
 */
fun ControlEntry.isCompactTile(deviceControls: Int, isMain: Boolean = false): Boolean {
    if (deviceControls < 2 || isMain) return false
    if (isVolume || volume != null) return false
    if (isButton || isProfile) return false
    return ControlAction.TURN_ON in actions && ControlAction.TURN_OFF in actions
}
