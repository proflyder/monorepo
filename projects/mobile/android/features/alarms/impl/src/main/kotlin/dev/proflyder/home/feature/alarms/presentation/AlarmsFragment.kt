package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.presentation.addBackCallback
import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.FlowKey
import dev.proflyder.home.core.presentation.FlowMember
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.UndoNotice
import dev.proflyder.home.core.presentation.flowViewModel
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
    ScreenFragment<FragmentAlarmsBinding, AlarmsUiState, AlarmsIntent, AlarmsEvent>(FragmentAlarmsBinding.Companion::inflate),
    FlowMember {
    override val flow: FlowKey = AlarmsFlow

    override val viewModel: AlarmsViewModel by tabViewModels()

    private val actions: AlarmsActionsViewModel by flowViewModel()

    private val adapter = AlarmAdapter(AlarmAdapterCallback(::dispatch) { actions.dispatch(it) })

    private val undo = UndoNotice(::requireView) { actions.dispatch(AlarmActionIntent.Undo) }

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        attach(actions, AlarmPrompts(this, actions::dispatch)::show)
    }

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
        binding.alarmsRefresh.setupPullToRefresh(binding.alarmsList) { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsRetry.setOnClickListener { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsOpenConnection.setOnClickListener { dispatch(AlarmsIntent.OpenConnection) }
        binding.alarmsAdd.setOnClickListener { dispatch(AlarmsIntent.Add) }
        addBackCallback(exitSelection)
        render(AlarmsUiState::list) { list -> renderList(states, list) }
        render(AlarmsUiState::rows, adapter::submitList)
        render(AlarmsUiState::selection) { selection -> renderSelection(selectionBars, selection) }
        render(actions, AlarmActionsState::undo) { deleted -> undo.render(deleted?.let { getString(R.string.alarms_deleted) }) }
    }

    override fun onBindingDestroyed() {
        undo.clear()
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
            is AlarmsEvent.ConfirmDeleteSelected -> confirmDeleteSelected(event.count)
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
