package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.graphics.Outline
import android.view.View
import android.view.ViewOutlineProvider
import dev.proflyder.home.core.designsystem.DeviceStateViews
import dev.proflyder.home.core.designsystem.bindAvailability
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceTileBinding

internal object OvalTileOutlineProvider : ViewOutlineProvider() {
    override fun getOutline(view: View, outline: Outline) {
        outline.setOval(0, 0, view.width, view.height)
    }
}

internal class DeviceTileBinder(
    private val tile: ItemControlDeviceTileBinding,
    private val dispatch: (ControlIntent) -> Unit,
) {
    private var device: ControlItem.Device? = null

    init {
        tile.controlTileRipple.clipToOutline = true
        tile.controlTileRipple.outlineProvider = OvalTileOutlineProvider
        listOf(tile.root, tile.controlTileRipple).forEach { target ->
            target.setOnClickListener { tap() }
            target.setOnLongClickListener { hold() }
        }
    }

    fun bind(device: ControlItem.Device) {
        this.device = device
        val entry = device.entry
        val context = tile.root.context
        val enabled = entry.enabledWhen(device.busy)
        tile.root.bindAvailability(entry.available)
        tile.root.isEnabled = enabled
        tile.controlTileRipple.isEnabled = enabled
        tile.root.contentDescription = context.getString(
            R.string.control_tile_description,
            entry.localizedTitle(context),
            entry.tileState(context),
        )
        tile.controlTileIcon.setImageResource(ControlIcons.resolve(entry))
        DeviceStateViews.bindIcon(tile.controlTileIcon, entry.isOn)
        DeviceStateViews.bindStatus(tile.controlTileLabel, entry.isOn && entry.available)
        tile.controlTileProgress.setVisible(device.busy)
        tile.controlTileLabel.text = entry.localizedTitle(context)
    }

    private fun tap() {
        val current = device?.takeIf { it.entry.enabledWhen(it.busy) } ?: return
        dispatch(ControlIntent.Command(DeviceCommand.Power(current.entry, !current.entry.isOn)))
    }

    private fun hold(): Boolean {
        device?.let { dispatch(ControlIntent.Hold(TapTarget.Item(it))) }
        return true
    }

    private fun ControlEntry.tileState(context: Context): String = context.getString(
        when {
            !available -> R.string.control_state_unavailable
            power == PowerState.ON -> R.string.control_state_on
            power == PowerState.OFF -> R.string.control_state_off
            else -> R.string.control_state_unknown
        },
    )
}
