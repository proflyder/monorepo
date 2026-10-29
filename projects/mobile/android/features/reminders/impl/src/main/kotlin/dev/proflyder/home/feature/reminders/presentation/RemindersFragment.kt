package dev.proflyder.home.feature.reminders.presentation

import android.os.Bundle
import androidx.fragment.app.viewModels
import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.impl.databinding.FragmentRemindersBinding

class RemindersFragment :
    BaseFragment<FragmentRemindersBinding>(FragmentRemindersBinding.Companion::inflate) {
    private val viewModel: RemindersViewModel by viewModels {
        viewModelFactory(RemindersViewModel::class.java)
    }

    private lateinit var adapter: ReminderAdapter

    /** The single callback instance shared with the adapter. */
    private val adapterCallback = object : BaseAdapterCallback<Schedule>(), ReminderAdapterCallback {
        override fun onItemClick(item: Schedule) = requireNavigator().navigate(ReminderEditorDestination(item.id))
        override fun onToggle(reminder: Schedule, enabled: Boolean) = viewModel.toggle(reminder, enabled)
        override fun onDelete(reminder: Schedule) = confirmDelete(reminder.id)
    }
    private lateinit var states: ListStateViews

    override fun onBindingCreated(binding: FragmentRemindersBinding, savedInstanceState: Bundle?) {
        adapter = ReminderAdapter(adapterCallback)
        binding.remindersList.layoutManager =
            GridLayoutManager(requireContext(), resources.getInteger(R.integer.reminders_columns))
        binding.remindersList.adapter = adapter

        states = ListStateViews(
            progress = binding.remindersProgress,
            empty = binding.remindersEmpty,
            error = binding.remindersError,
            content = binding.remindersList,
            errorText = binding.remindersErrorText,
        )
        binding.remindersRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.remindersRetry.setOnClickListener { viewModel.refresh() }
        binding.remindersAdd.setOnClickListener { requireNavigator().navigate(ReminderEditorDestination()) }

        viewModel.state.observe { render(it) }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun render(state: RemindersUiState) {
        val hasItems = state.reminders.isNotEmpty()
        val loadError = state.loadError
        when {
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> states.showError(loadError.userMessage(requireContext()))
            !hasItems -> states.showEmpty()
            else -> {
                states.showContent()
                adapter.submitList(state.reminders)
            }
        }
        binding.remindersRefresh.isRefreshing = state.refreshing && hasItems
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

    private fun confirmDelete(id: String) {
        confirm(R.string.reminders_delete_confirm, R.string.reminders_delete_yes, R.string.reminders_cancel) {
            viewModel.state.value.reminders.firstOrNull { it.id == id }?.let(viewModel::delete)
        }
    }
}
