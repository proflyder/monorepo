package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlRole

/** The only actions a plain on/off control may accept. */
private val BINARY_ACTIONS = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)

/**
 * One pure rule deciding how a control inside a device card is drawn: a round
 * compact tile or a normal row.
 *
 * A tile is a plain binary on/off control: its only actions are turn_on and
 * turn_off, it carries no value (no volume, no adjustable light), and it is not
 * a secondary setting. The card must hold at least two controls, because a
 * single-control card keeps the ordinary device row. There is no list of
 * known kinds: a new binary kind with the same capabilities becomes a tile
 * automatically, and anything else stays a row.
 */
fun ControlEntry.isCompactTile(deviceControls: Int): Boolean {
    if (deviceControls < 2) return false
    if (role == ControlRole.SETTING) return false
    if (volume != null || light != null) return false
    return actions.isNotEmpty() && actions.all { it in BINARY_ACTIONS }
}
