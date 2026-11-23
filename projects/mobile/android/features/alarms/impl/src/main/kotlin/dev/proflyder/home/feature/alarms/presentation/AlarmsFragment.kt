package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.presentation.addBackCallback
import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.presentation.tabViewModels
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding

internal class AlarmsFragment :
    ScreenFragment<FragmentAlarmsBinding, AlarmsUiState, AlarmsIntent, AlarmsEvent>(FragmentAlarmsBinding.Companion::inflate) {
    override val viewModel: AlarmsViewModel by tabViewModels()

    private val adapter = AlarmAdapter(AlarmAdapterCallback(::dispatch))

    private val exitSelection = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = dispatch(AlarmsIntent.ClearSelection)
    }

    override fun onViewReady(binding: FragmentAlarmsBinding, savedInstanceState: Bundle?) {
        binding.alarmsList.applyScrollBottomInset(binding.alarmsList.fabClearance())
        binding.alarmsAdd.liftAboveBottomInsets()
        binding.alarmsActionBar.liftAboveBottomInsets()
        binding.alarmsList.adapter = adapter
        val states = ListStateViews(
            progress = binding.alarmsProgress,
            empty = binding.alarmsEmpty,
            error = binding.alarmsError,
            content = binding.alarmsList,
            errorText = binding.alarmsErrorText,
        )
        val selectionBars = AlarmSelectionBars(binding, ::dispatch)
        binding.alarmsRefresh.setupPullToRefresh { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsRetry.setOnClickListener { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsOpenConnection.setOnClickListener { dispatch(AlarmsIntent.OpenConnection) }
        binding.alarmsAdd.setOnClickListener { dispatch(AlarmsIntent.Add) }
        addBackCallback(exitSelection)
        render(AlarmsUiState::list) { list -> renderList(states, list) }
        render(AlarmsUiState::rows, adapter::submitList)
        render(AlarmsUiState::selection) { selection -> renderSelection(selectionBars, selection) }
    }

    override fun onBindingDestroyed() {
        binding.alarmsList.adapter = null
    }

    private fun renderList(states: ListStateViews, list: AlarmsListUi) {
        states.render(list.load, list.hasItems) { it.userMessage(requireContext()) }
        binding.alarmsOpenConnection.setVisible(list.load.failure == RemoteFailure.NotConfigured)
        binding.alarmsRefresh.isRefreshing = list.load.refreshing && list.hasItems
    }

    private fun renderSelection(bars: AlarmSelectionBars, selection: AlarmSelectionUi) {
        binding.alarmsRefresh.isEnabled = !selection.active
        exitSelection.isEnabled = selection.active
        bars.render(selection)
    }

    override fun onEvent(event: AlarmsEvent) {
        when (event) {
            is AlarmsEvent.Failure -> showFailure(event.failure)
            is AlarmsEvent.OpenRoute -> navigate(event.route)
            is AlarmsEvent.AskDisable -> askDisable(event.id)
            is AlarmsEvent.ConfirmDelete -> confirmDelete(event.id)
            is AlarmsEvent.ConfirmDeleteSelected -> confirmDeleteSelected(event.count)
        }
    }

    private fun askDisable(id: String) {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.alarms_disable_title)
            .setMessage(R.string.alarms_disable_message)
            .setNeutralButton(R.string.alarms_disable_skip_once) { _, _ -> dispatch(AlarmsIntent.SkipOnce(id)) }
            .setPositiveButton(R.string.alarms_disable_turn_off) { _, _ -> dispatch(AlarmsIntent.TurnOff(id)) }
            .setNegativeButton(DesignR.string.ds_action_cancel, null)
            .show()
    }

    private fun confirmDelete(id: String) {
        confirm(R.string.alarms_delete_confirm, DesignR.string.ds_action_delete, DesignR.string.ds_action_cancel) {
            dispatch(AlarmsIntent.Delete(id))
        }
    }

    private fun confirmDeleteSelected(count: Int) {
        confirm(
            resources.getQuantityString(R.plurals.alarms_delete_selected_confirm, count, count),
            DesignR.string.ds_action_delete,
            DesignR.string.ds_action_cancel,
        ) { dispatch(AlarmsIntent.DeleteSelected) }
    }
}
