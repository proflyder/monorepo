package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.ui.animateBar
import dev.proflyder.home.core.ui.animateVerticalPadding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding

internal class AlarmSelectionBars(
    private val binding: FragmentAlarmsBinding,
    dispatch: (AlarmsIntent) -> Unit,
) {
    private val listBaseTopPadding = binding.alarmsList.paddingTop
    private var shown: Boolean? = null

    init {
        binding.alarmsSelectionClose.setOnClickListener { dispatch(AlarmsIntent.ClearSelection) }
        binding.alarmsSelectionAll.setOnClickListener { dispatch(AlarmsIntent.ToggleSelectAll) }
        binding.alarmsSelectionEnable.setOnClickListener { dispatch(AlarmsIntent.SetSelectedEnabled(true)) }
        binding.alarmsSelectionDisable.setOnClickListener { dispatch(AlarmsIntent.SetSelectedEnabled(false)) }
        binding.alarmsSelectionDelete.setOnClickListener { dispatch(AlarmsIntent.RequestDeleteSelected) }
    }

    fun render(selection: AlarmSelectionUi) {
        if (shown != selection.active) showBars(selection.active, animate = shown != null)
        renderContent(selection)
    }

    private fun showBars(selecting: Boolean, animate: Boolean) {
        shown = selecting
        val topBarHeight = binding.alarmsSelectionBar.layoutParams.height
        binding.alarmsSelectionBar.animateBar(selecting, -topBarHeight.toFloat(), animate)
        binding.alarmsActionBar.animateBar(selecting, bottomBarHeight().toFloat(), animate)
        binding.alarmsList.animateVerticalPadding(
            topPx = listBaseTopPadding + if (selecting) topBarHeight else 0,
            bottomPx = binding.alarmsList.paddingBottom,
            entering = selecting,
            animate = animate,
        )
        if (selecting) binding.alarmsAdd.hide() else binding.alarmsAdd.show()
    }

    private fun bottomBarHeight(): Int = binding.alarmsActionBar.height.takeIf { it > 0 }
        ?: binding.root.resources.getDimensionPixelSize(R.dimen.alarms_action_bar_height)

    private fun renderContent(selection: AlarmSelectionUi) = with(binding) {
        val count = selection.count
        alarmsSelectionCount.text = root.resources.getQuantityString(R.plurals.alarms_selected_count, count, count)
        alarmsSelectionAll.setText(if (selection.allSelected) R.string.alarms_select_none else R.string.alarms_select_all)
        val actions = selection.actions
        alarmsSelectionEnable.setVisible(actions.canEnable)
        alarmsSelectionDisable.setVisible(actions.canDisable)
        listOf(alarmsSelectionEnable, alarmsSelectionDisable, alarmsSelectionDelete).forEach { it.isEnabled = !selection.busy }
    }
}
