package dev.proflyder.home.feature.reminders.presentation

import android.os.Bundle
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.impl.databinding.FragmentRemindersBinding

internal class RemindersFragment :
    ScreenFragment<FragmentRemindersBinding, RemindersUiState, RemindersIntent, RemindersEvent>(
        FragmentRemindersBinding.Companion::inflate,
    ) {
    override val viewModel: RemindersViewModel by activityViewModels { viewModelFactory(RemindersViewModel::class.java) }

    private val adapter = ReminderAdapter(ReminderAdapterCallback(::dispatch))
    private var states: ListStateViews? = null

    override fun onScreenCreated(binding: FragmentRemindersBinding, savedInstanceState: Bundle?) {
        binding.remindersList.applyScrollBottomInset(binding.remindersList.fabClearance())
        binding.remindersAdd.liftAboveBottomInsets()
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
        binding.remindersRefresh.setupPullToRefresh { dispatch(RemindersIntent.Refresh(RefreshMode.HARD)) }
        binding.remindersRetry.setOnClickListener { dispatch(RemindersIntent.Refresh(RefreshMode.HARD)) }
        binding.remindersAdd.setOnClickListener { dispatch(RemindersIntent.Add) }
    }

    override fun onBindingDestroyed() {
        binding.remindersList.adapter = null
        states = null
    }

    override fun render(state: RemindersUiState) {
        val hasItems = state.reminders.isNotEmpty()
        states?.render(state.load, hasItems) { it.userMessage(requireContext()) }
        adapter.submitList(state.reminders)
        binding.remindersRefresh.isRefreshing = state.load.refreshing && hasItems
    }

    override fun onEvent(event: RemindersEvent) {
        when (event) {
            is RemindersEvent.Failure -> showFailure(event.failure)
            is RemindersEvent.OpenEditor -> requireNavigator().navigate(ReminderEditorDestination(event.id))
            is RemindersEvent.ConfirmDelete -> confirmDelete(event.id)
        }
    }

    private fun confirmDelete(id: String) {
        confirm(R.string.reminders_delete_confirm, R.string.reminders_delete_yes, R.string.reminders_cancel) {
            dispatch(RemindersIntent.Delete(id))
        }
    }
}
