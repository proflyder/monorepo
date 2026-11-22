package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.recyclerview.widget.GridLayoutManager
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.animateBar
import dev.proflyder.home.core.ui.animateVerticalPadding
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.injectedActivityViewModels
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding

class AlarmsFragment : BaseFragment<FragmentAlarmsBinding>(FragmentAlarmsBinding.Companion::inflate) {
    internal val viewModel: AlarmsViewModel by injectedActivityViewModels()

    private lateinit var adapter: AlarmAdapter
    private lateinit var states: ListStateViews

    private var listBaseTopPadding = 0

    private var selectionBarsShown: Boolean? = null

    private val adapterCallback = object : BaseAdapterCallback<AlarmRow>(), AlarmAdapterCallback {
        override fun onItemClick(item: AlarmRow) = onAlarmClick(item.alarm)
        override fun onItemLongClick(item: AlarmRow) = viewModel.toggleSelection(item.alarm.id)
        override fun onToggle(alarm: Schedule, enabled: Boolean) = requestToggle(alarm, enabled)
        override fun onSnooze(alarm: Schedule) = viewModel.snooze(alarm)
        override fun onStop(alarm: Schedule) = viewModel.stop(alarm)
        override fun onDelete(alarm: Schedule) = confirmDelete(alarm.id)
    }

    private val exitSelection = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = viewModel.clearSelection()
    }

    override fun onBindingCreated(binding: FragmentAlarmsBinding, savedInstanceState: Bundle?) {
        binding.alarmsList.applyScrollBottomInset(binding.alarmsList.fabClearance())
        binding.alarmsAdd.liftAboveBottomInsets()
        binding.alarmsActionBar.liftAboveBottomInsets()
        adapter = AlarmAdapter(adapterCallback)
        binding.alarmsList.layoutManager =
            GridLayoutManager(requireContext(), resources.getInteger(R.integer.alarms_columns))
        binding.alarmsList.adapter = adapter
        listBaseTopPadding = binding.alarmsList.paddingTop
        selectionBarsShown = null

        states = ListStateViews(
            progress = binding.alarmsProgress,
            empty = binding.alarmsEmpty,
            error = binding.alarmsError,
            content = binding.alarmsList,
            errorText = binding.alarmsErrorText,
        )

        binding.alarmsRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.alarmsRetry.setOnClickListener { viewModel.refresh() }
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
        viewModel.refresh(silent = true)
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
            adapter.notifyItemRangeChanged(0, adapter.itemCount)
            viewModel.consumeActionError()
        }
    }

    private fun renderSelection(state: AlarmsUiState) {
        val selecting = state.selectionMode
        exitSelection.isEnabled = selecting

        if (selectionBarsShown != selecting) {
            val animate = selectionBarsShown != null
            selectionBarsShown = selecting

            val topBarHeight = binding.alarmsSelectionBar.layoutParams.height
            binding.alarmsSelectionBar.animateBar(
                visible = selecting,
                hiddenTranslationY = -topBarHeight.toFloat(),
                animate = animate,
            )

            val bottomBarHeight = binding.alarmsActionBar.height
                .takeIf { it > 0 }
                ?: resources.getDimensionPixelSize(R.dimen.alarms_action_bar_height)
            binding.alarmsActionBar.animateBar(
                visible = selecting,
                hiddenTranslationY = bottomBarHeight.toFloat(),
                animate = animate,
            )

            binding.alarmsList.animateVerticalPadding(
                topPx = listBaseTopPadding + if (selecting) topBarHeight else 0,
                bottomPx = binding.alarmsList.paddingBottom,
                entering = selecting,
                animate = animate,
            )

            if (selecting) binding.alarmsAdd.hide() else binding.alarmsAdd.show()
        }

        val count = state.selectedIds.size
        binding.alarmsSelectionCount.text =
            resources.getQuantityString(R.plurals.alarms_selected_count, count, count)
        binding.alarmsSelectionAll.setText(
            if (state.allSelected) R.string.alarms_select_none else R.string.alarms_select_all,
        )
        val selectionActions = alarmSelectionActions(state.alarms, state.selectedIds)
        binding.alarmsSelectionEnable.setVisible(selectionActions.canEnable)
        binding.alarmsSelectionDisable.setVisible(selectionActions.canDisable)

        val actionsEnabled = !state.busy
        binding.alarmsSelectionEnable.isEnabled = actionsEnabled
        binding.alarmsSelectionDisable.isEnabled = actionsEnabled
        binding.alarmsSelectionDelete.isEnabled = actionsEnabled
    }

    internal fun onAlarmClick(alarm: Schedule) {
        if (viewModel.state.value.selectionMode) {
            viewModel.toggleSelection(alarm.id)
        } else {
            requireNavigator().navigate(AlarmEditorDestination(alarm.id))
        }
    }

    internal fun requestToggle(alarm: Schedule, enabled: Boolean) {
        when {
            enabled -> viewModel.setEnabled(alarm, true)
            alarmDisableChoice(alarm) == AlarmDisableChoice.DISABLE -> viewModel.setEnabled(alarm, false)
            else -> showDisableChoice(alarm)
        }
    }

    private fun showDisableChoice(alarm: Schedule) {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.alarms_disable_title)
            .setMessage(R.string.alarms_disable_message)
            .setNeutralButton(R.string.alarms_disable_skip_once) { _, _ -> viewModel.skipNextOccurrence(alarm) }
            .setPositiveButton(R.string.alarms_disable_turn_off) { _, _ -> viewModel.setEnabled(alarm, false) }
            .setNegativeButton(R.string.alarms_cancel) { _, _ -> restoreSwitch(alarm) }
            .setOnCancelListener { restoreSwitch(alarm) }
            .show()
    }

    private fun restoreSwitch(alarm: Schedule) {
        val position = viewModel.state.value.alarms.indexOfFirst { it.id == alarm.id }
        if (position >= 0) adapter.notifyItemChanged(position)
    }

    internal fun confirmDelete(id: String) {
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
