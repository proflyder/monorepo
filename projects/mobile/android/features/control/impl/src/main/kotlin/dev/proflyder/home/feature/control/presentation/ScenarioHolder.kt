package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.designsystem.bindAvailability
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

internal class ScenarioHolder(
    private val views: ItemControlScenarioBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    init {
        forwardRowClicks()
    }

    override fun bind(item: ControlItem) {
        val row = item as ControlItem.Scenario
        views.controlScenarioTitle.text = row.entry.localizedTitle(views.root.context)
        views.controlScenarioIcon.setImageResource(ControlIcons.resolve(row.entry))
        views.controlScenarioProgress.setVisible(row.busy)
        views.root.bindAvailability(row.entry.available)
        views.root.isEnabled = row.entry.available && !row.busy
    }
}
