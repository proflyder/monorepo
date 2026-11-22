package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.designsystem.bindChevron
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHeaderBinding

internal class HeaderHolder(
    private val views: ItemControlHeaderBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    init {
        views.root.setOnClickListener { toggle() }
    }

    override fun bind(item: ControlItem) {
        val header = item as ControlItem.Header
        views.controlHeaderTitle.text = header.title
        views.controlHeaderSummary.text = header.summary.orEmpty()
        views.controlHeaderSummary.setVisible(header.summary != null)
        views.controlHeaderChevron.setVisible(header.collapsible)
        views.controlHeaderChevron.bindChevron(expanded = !header.collapsed)
    }

    private fun toggle() {
        val header = item as ControlItem.Header
        if (header.collapsible) callback.dispatch(ControlIntent.ToggleRoom(header.section))
    }
}
