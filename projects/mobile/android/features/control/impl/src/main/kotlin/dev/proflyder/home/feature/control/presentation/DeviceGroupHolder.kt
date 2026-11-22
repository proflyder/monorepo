package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import dev.proflyder.home.core.designsystem.DeviceStateViews
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceGroupBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceRowBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceTileBinding
import dev.proflyder.home.feature.control.isCompactTile

internal class DeviceGroupHolder(
    private val views: ItemControlDeviceGroupBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    init {
        views.controlGroupSettings.setOnClickListener { toggleSettings() }
    }

    override fun bind(item: ControlItem) {
        val group = item as ControlItem.DeviceGroup
        val titled = group.primary.size != 1
        views.controlGroupTitle.text = group.title
        views.controlGroupTitle.setVisible(titled)
        alignPrimary(titled)
        padBottom(group.settings.isEmpty())
        DeviceStateViews.bindCard(views.root, group.primary.any { it.entry.isOn })
        val layout = GroupLayout.of(group)
        views.controlGroupPrimary.setVisible(layout.rows.isNotEmpty())
        syncRows(views.controlGroupPrimary, layout.rows, group.statesKnown)
        syncTiles(views.controlGroupTiles, layout.tiles, group.statesKnown, visible = layout.tiles.isNotEmpty())
        syncRows(views.controlGroupActions, group.actions, group.statesKnown)
        views.controlGroupActions.setVisible(group.actions.isNotEmpty())
        bindSettings(group, layout)
    }

    private fun bindSettings(group: ControlItem.DeviceGroup, layout: GroupLayout) {
        views.controlGroupSettings.text = views.root.context.getString(R.string.control_device_settings, group.settings.size)
        views.controlGroupSettings.setVisible(group.settings.isNotEmpty())
        syncTiles(views.controlGroupSettingsTiles, layout.settingTiles, group.statesKnown, layout.settingTiles.isNotEmpty())
        syncRows(views.controlGroupSettingsList, layout.settingRows, group.statesKnown)
        views.controlGroupSettingsList.setVisible(layout.settingRows.isNotEmpty())
    }

    private fun syncRows(container: ViewGroup, rows: List<ControlItem.DeviceGroup.Row>, statesKnown: Boolean) =
        container.bindChildren(rows, ::createRow) { binder, row -> binder.bindGroupRow(row, statesKnown) }

    private fun syncTiles(
        container: ViewGroup,
        tiles: List<ControlItem.DeviceGroup.Row>,
        statesKnown: Boolean,
        visible: Boolean,
    ) {
        container.setVisible(visible)
        container.bindChildren(tiles, ::createTile) { binder, row ->
            binder.bind(ControlItem.Device(row.entry, row.busy, statesKnown))
        }
    }

    private fun createRow(parent: ViewGroup): Pair<View, GroupRowBinder> {
        val row = ItemControlDeviceRowBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return row.root to GroupRowBinder(row, callback.dispatch)
    }

    private fun createTile(parent: ViewGroup): Pair<View, DeviceTileBinder> {
        val tile = ItemControlDeviceTileBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return tile.root to DeviceTileBinder(tile, callback.dispatch)
    }

    private fun toggleSettings() {
        callback.dispatch(ControlIntent.ToggleDevice((item as ControlItem.DeviceGroup).deviceId))
    }

    private fun alignPrimary(titled: Boolean) {
        val params = views.controlGroupPrimary.layoutParams as ViewGroup.MarginLayoutParams
        params.topMargin = views.root.resources.getDimensionPixelSize(
            if (titled) R.dimen.control_group_title_gap else R.dimen.control_group_untitled_gap,
        )
        views.controlGroupPrimary.layoutParams = params
    }

    private fun padBottom(plain: Boolean) {
        val body = views.controlGroupBody
        val bottom = views.root.resources.getDimensionPixelSize(
            if (plain) R.dimen.control_card_padding_bottom_plain else R.dimen.control_card_padding_bottom_with_settings,
        )
        body.setPadding(body.paddingLeft, body.paddingTop, body.paddingRight, bottom)
    }
}

internal data class GroupLayout(
    val rows: List<ControlItem.DeviceGroup.Row>,
    val tiles: List<ControlItem.DeviceGroup.Row>,
    val settingRows: List<ControlItem.DeviceGroup.Row>,
    val settingTiles: List<ControlItem.DeviceGroup.Row>,
) {
    companion object {
        fun of(group: ControlItem.DeviceGroup): GroupLayout {
            val controls = group.primary.size + group.settings.size + group.actions.size
            val (tiles, rows) = group.primary.partition {
                it.entry.isCompactTile(controls, isMain = it.entry.id == group.mainId)
            }
            val shown = group.settings.takeIf { group.expanded }.orEmpty()
            val (settingTiles, settingRows) = shown.partition { it.entry.isCompactTile(controls) }
            return GroupLayout(
                rows = rows.sortedBy { if (it.entry.id == group.mainId) 0 else 1 },
                tiles = tiles,
                settingRows = settingRows,
                settingTiles = settingTiles,
            )
        }
    }
}

internal class GroupRowBinder(
    private val views: ItemControlDeviceRowBinding,
    dispatch: (ControlIntent) -> Unit,
) {
    private val row = EntryRowBinder(
        EntryRowViews(
            root = views.root,
            icon = views.controlRowIcon,
            state = views.controlRowState,
            switch = views.controlRowSwitch,
            progress = views.controlRowProgress,
        ),
        dispatch,
    )

    fun bindGroupRow(groupRow: ControlItem.DeviceGroup.Row, statesKnown: Boolean) {
        val entry = groupRow.entry
        val context = views.root.context
        views.controlRowTitle.text = entry.localizedTitle(context).takeUnless { it.isBlank() || it == entry.id }
            ?: entry.device.ifBlank { entry.localizedTitle(context) }
        val device = ControlItem.Device(entry, groupRow.busy, statesKnown)
        row.bind(device.toRow(hold = ControlIntent.AskPin(entry)))
    }
}
