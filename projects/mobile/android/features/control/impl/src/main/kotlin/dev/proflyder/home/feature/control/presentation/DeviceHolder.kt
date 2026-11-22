package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.designsystem.DeviceStateViews
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceBinding

internal class DeviceHolder(
    private val views: ItemControlDeviceBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    private val row = EntryRowBinder(
        EntryRowViews(
            root = views.root,
            icon = views.controlDeviceIcon,
            state = views.controlDeviceState,
            switch = views.controlDeviceSwitch,
            progress = views.controlDeviceProgress,
            card = { on -> DeviceStateViews.bindCard(views.root, on) },
        ),
        callback.dispatch,
    )

    override fun bind(item: ControlItem) {
        val device = item as ControlItem.Device
        views.controlDeviceTitle.text = device.entry.localizedTitle(views.root.context)
        row.bind(device.toRow())
    }
}

internal fun ControlItem.Device.toRow(hold: ControlIntent = ControlIntent.Hold(TapTarget.Item(this))): EntryRow =
    EntryRow(
        entry = entry,
        busy = busy,
        tap = when {
            entry.isButton -> ControlIntent.Command(DeviceCommand.Press(entry))
            else -> ControlIntent.Tap(TapTarget.Item(this))
        },
        hold = hold,
    )
