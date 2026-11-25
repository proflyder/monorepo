package dev.proflyder.home.feature.reminders.presentation

import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.presentation.tabViewModels
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.impl.databinding.FragmentRemindersBinding
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.fabContentInsets

internal class RemindersFragment :
    ScreenFragment<FragmentRemindersBinding, RemindersUiState, RemindersIntent, RemindersEvent>(
        FragmentRemindersBinding.Companion::inflate,
    ) {
    override val viewModel: RemindersViewModel by tabViewModels()

    private val adapter = ReminderAdapter(ReminderAdapterCallback(::dispatch))
    override fun contentInsets(): ContentInsets = fabContentInsets()

    override fun onViewReady(binding: FragmentRemindersBinding, savedInstanceState: Bundle?) {
        binding.remindersAdd.liftAboveBottomInsets()
        binding.remindersList.adapter = adapter
        val states = ListStateViews(
            progress = binding.remindersProgress,
            empty = binding.remindersEmpty,
            error = binding.remindersError,
            content = binding.remindersList,
            errorText = binding.remindersErrorText,
        )
        binding.remindersRefresh.setupPullToRefresh(binding.remindersList) { dispatch(RemindersIntent.Refresh(RefreshMode.HARD)) }
        binding.remindersRetry.setOnClickListener { dispatch(RemindersIntent.Refresh(RefreshMode.HARD)) }
        binding.remindersAdd.setOnClickListener { dispatch(RemindersIntent.Add) }
        render({ it.load to it.reminders.isNotEmpty() }) { (load, hasItems) -> renderLoad(states, load, hasItems) }
        render(RemindersUiState::reminders, adapter::submitList)
    }

    override fun onBindingDestroyed() {
        binding.remindersList.adapter = null
    }

    private fun renderLoad(states: ListStateViews, load: ListLoadState, hasItems: Boolean) {
        states.render(load, hasItems) { it.userMessage(requireContext()) }
        binding.remindersRefresh.isRefreshing = load.refreshing && hasItems
    }

    override fun onEvent(event: RemindersEvent) {
        when (event) {
            is RemindersEvent.Failure -> showFailure(event.failure)
            is RemindersEvent.OpenRoute -> navigate(event.route)
            is RemindersEvent.ConfirmDelete -> confirmDelete(event.id)
        }
    }

    private fun confirmDelete(id: String) {
        confirm(R.string.reminders_delete_confirm, DesignR.string.ds_action_delete, DesignR.string.ds_action_cancel) {
            dispatch(RemindersIntent.Delete(id))
        }
    }
}
