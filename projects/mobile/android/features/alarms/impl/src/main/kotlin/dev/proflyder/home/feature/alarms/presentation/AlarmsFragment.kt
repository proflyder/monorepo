package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.fragment.app.viewModels
import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding

class AlarmsFragment : BaseFragment<FragmentAlarmsBinding>(FragmentAlarmsBinding.Companion::inflate) {
    private val viewModel: AlarmsViewModel by viewModels {
        viewModelFactory(AlarmsViewModel::class.java)
    }

    private lateinit var adapter: AlarmAdapter
    private lateinit var states: ListStateViews

    /** The single callback instance shared with the adapter. */
    private val adapterCallback = object : BaseAdapterCallback<AlarmRow>(), AlarmAdapterCallback {
        override fun onItemClick(item: AlarmRow) = onAlarmClick(item.alarm)
        override fun onItemLongClick(item: AlarmRow) = viewModel.toggleSelection(item.alarm.id)
        override fun onToggle(alarm: Schedule, enabled: Boolean) = viewModel.setEnabled(alarm, enabled)
        override fun onSnooze(alarm: Schedule) = viewModel.snooze(alarm)
        override fun onStop(alarm: Schedule) = viewModel.stop(alarm)
        override fun onDelete(alarm: Schedule) = confirmDelete(alarm.id)
    }

    /** Back leaves multi-select first; enabled only while something is selected. */
    private val exitSelection = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = viewModel.clearSelection()
    }

    override fun onBindingCreated(binding: FragmentAlarmsBinding, savedInstanceState: Bundle?) {
        adapter = AlarmAdapter(adapterCallback)
        binding.alarmsList.layoutManager =
            GridLayoutManager(requireContext(), resources.getInteger(R.integer.alarms_columns))
        binding.alarmsList.adapter = adapter

        states = ListStateViews(
            progress = binding.alarmsProgress,
            empty = binding.alarmsEmpty,
            error = binding.alarmsError,
            content = binding.alarmsList,
            errorText = binding.alarmsErrorText,
        )

        binding.alarmsRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.alarmsRetry.setOnClickListener { viewModel.refresh() }
        // With no endpoint/token configured the server is intentionally
        // unreachable; make the state actionable instead of a dead empty list.
        binding.alarmsOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }
        binding.alarmsAdd.setOnClickListener { requireNavigator().navigate(AlarmEditorDestination()) }

        binding.alarmsSelectionClose.setOnClickListener { viewModel.clearSelection() }
        binding.alarmsSelectionAll.setOnClickListener {
            if (viewModel.state.value.allSelected) viewModel.clearSelection() else viewModel.selectAll()
        }
        binding.alarmsSelectionEnable.setOnClickListener { viewModel.setSelectedEnabled(true) }
        binding.alarmsSelectionDisable.setOnClickListener { viewModel.setSelectedEnabled(false) }
        binding.alarmsSelectionDelete.setOnClickListener { confirmDeleteSelected() }

        requireActivity().onBackPressedDispatcher.addCallback(viewLifecycleOwner, exitSelection)

        viewModel.state.observe { render(it) }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun render(state: AlarmsUiState) {
        val hasItems = state.alarms.isNotEmpty()
        val loadError = state.loadError
        when {
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> {
                states.showError(loadError.userMessage(requireContext()))
                binding.alarmsOpenConnection.setVisible(loadError == RemoteFailure.NotConfigured)
            }
            !hasItems -> states.showEmpty()
            else -> {
                // Keep the existing list visible while a refresh is in flight;
                // do not replace it with a spinner.
                states.showContent()
                adapter.submitList(
                    state.alarms.map { AlarmRow(it, it.id in state.selectedIds, state.selectionMode) },
                )
            }
        }
        binding.alarmsRefresh.isRefreshing = state.refreshing && hasItems
        binding.alarmsRefresh.isEnabled = !state.selectionMode
        renderSelection(state)

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            // A switch may have moved optimistically; re-render server-confirmed values.
            adapter.notifyDataSetChanged()
            viewModel.consumeActionError()
        }
    }

    private fun renderSelection(state: AlarmsUiState) {
        val selecting = state.selectionMode
        exitSelection.isEnabled = selecting
        binding.alarmsSelectionBar.setVisible(selecting)
        binding.alarmsActionBar.setVisible(selecting)
        if (selecting) binding.alarmsAdd.hide() else binding.alarmsAdd.show()

        val count = state.selectedIds.size
        binding.alarmsSelectionCount.text =
            resources.getQuantityString(R.plurals.alarms_selected_count, count, count)
        binding.alarmsSelectionAll.setText(
            if (state.allSelected) R.string.alarms_select_none else R.string.alarms_select_all,
        )
        val actionsEnabled = !state.busy
        binding.alarmsSelectionEnable.isEnabled = actionsEnabled
        binding.alarmsSelectionDisable.isEnabled = actionsEnabled
        binding.alarmsSelectionDelete.isEnabled = actionsEnabled
    }

    private fun onAlarmClick(alarm: Schedule) {
        if (viewModel.state.value.selectionMode) {
            viewModel.toggleSelection(alarm.id)
        } else {
            requireNavigator().navigate(AlarmEditorDestination(alarm.id))
        }
    }

    private fun confirmDelete(id: String) {
        confirm(R.string.alarms_delete_confirm, R.string.alarms_delete_yes, R.string.alarms_cancel) {
            viewModel.state.value.alarms.firstOrNull { it.id == id }?.let(viewModel::delete)
        }
    }

    private fun confirmDeleteSelected() {
        val count = viewModel.state.value.selectedIds.size
        if (count == 0) return
        confirm(
            resources.getQuantityString(R.plurals.alarms_delete_selected_confirm, count, count),
            R.string.alarms_delete_yes,
            R.string.alarms_cancel,
        ) { viewModel.deleteSelected() }
    }
}
