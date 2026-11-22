package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.GridLayoutManager
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsBinding

internal class AlarmsFragment :
    ScreenFragment<FragmentAlarmsBinding, AlarmsUiState, AlarmsIntent, AlarmsEvent>(FragmentAlarmsBinding.Companion::inflate) {
    override val viewModel: AlarmsViewModel by activityViewModels { viewModelFactory(AlarmsViewModel::class.java) }

    private val adapter = AlarmAdapter(AlarmAdapterCallback(::dispatch))
    private var states: ListStateViews? = null
    private var selectionBars: AlarmSelectionBars? = null

    private val exitSelection = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = dispatch(AlarmsIntent.ClearSelection)
    }

    override fun onScreenCreated(binding: FragmentAlarmsBinding, savedInstanceState: Bundle?) {
        binding.alarmsList.applyScrollBottomInset(binding.alarmsList.fabClearance())
        binding.alarmsAdd.liftAboveBottomInsets()
        binding.alarmsActionBar.liftAboveBottomInsets()
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
        selectionBars = AlarmSelectionBars(binding, ::dispatch)
        binding.alarmsRefresh.setupPullToRefresh { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsRetry.setOnClickListener { dispatch(AlarmsIntent.Refresh(RefreshMode.HARD)) }
        binding.alarmsOpenConnection.setOnClickListener { dispatch(AlarmsIntent.OpenConnection) }
        binding.alarmsAdd.setOnClickListener { dispatch(AlarmsIntent.Add) }
        requireActivity().onBackPressedDispatcher.addCallback(viewLifecycleOwner, exitSelection)
    }

    override fun onBindingDestroyed() {
        binding.alarmsList.adapter = null
        states = null
        selectionBars = null
    }

    override fun render(state: AlarmsUiState) {
        val hasItems = state.alarms.isNotEmpty()
        states?.render(state.load, hasItems) { it.userMessage(requireContext()) }
        binding.alarmsOpenConnection.setVisible(state.load.failure == RemoteFailure.NotConfigured)
        adapter.submitList(state.rows)
        binding.alarmsRefresh.isRefreshing = state.load.refreshing && hasItems
        binding.alarmsRefresh.isEnabled = !state.selectionMode
        exitSelection.isEnabled = state.selectionMode
        selectionBars?.render(state)
    }

    override fun onEvent(event: AlarmsEvent) {
        when (event) {
            is AlarmsEvent.Failure -> showFailure(event.failure)
            is AlarmsEvent.OpenEditor -> requireNavigator().navigate(AlarmEditorDestination(event.id))
            is AlarmsEvent.AskDisable -> askDisable(event.id)
            AlarmsEvent.OpenConnection -> requireNavigator().selectTab(CONNECTION_TAB)
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
            .setNegativeButton(R.string.alarms_cancel, null)
            .show()
    }

    private fun confirmDelete(id: String) {
        confirm(R.string.alarms_delete_confirm, R.string.alarms_delete_yes, R.string.alarms_cancel) {
            dispatch(AlarmsIntent.Delete(id))
        }
    }

    private fun confirmDeleteSelected(count: Int) {
        confirm(
            resources.getQuantityString(R.plurals.alarms_delete_selected_confirm, count, count),
            R.string.alarms_delete_yes,
            R.string.alarms_cancel,
        ) { dispatch(AlarmsIntent.DeleteSelected) }
    }

    private companion object {
        const val CONNECTION_TAB = "connection"
    }
}
