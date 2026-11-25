package dev.proflyder.home.feature.reminders.presentation

import dev.proflyder.home.core.presentation.bindSwitch
import dev.proflyder.home.core.presentation.bindText
import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.EditorStatus
import dev.proflyder.home.core.ui.injectedViewModels
import dev.proflyder.home.core.ui.keepFocusedVisible
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.impl.databinding.FragmentReminderEditorBinding
import java.time.LocalDate

internal class ReminderEditorFragment :
    ScreenFragment<FragmentReminderEditorBinding, ReminderEditorUiState, ReminderEditorIntent, ReminderEditorEvent>(
        FragmentReminderEditorBinding.Companion::inflate,
    ) {
    override val viewModel: ReminderEditorViewModel by injectedViewModels()

    override fun onViewReady(binding: FragmentReminderEditorBinding, savedInstanceState: Bundle?) {
        binding.root.keepFocusedVisible()
        val label = bindText(binding.reminderEditorLabel, ReminderEditorIntent::SetLabel)
        val enabled = bindSwitch(binding.reminderEditorEnabled, ReminderEditorIntent::SetEnabled)
        binding.reminderEditorDate.setOnClickListener { dispatch(ReminderEditorIntent.PickDate) }
        binding.reminderEditorTime.setOnClickListener { dispatch(ReminderEditorIntent.PickTime) }
        binding.reminderEditorSave.setOnClickListener { dispatch(ReminderEditorIntent.Save) }
        binding.reminderEditorDelete.setOnClickListener { dispatch(ReminderEditorIntent.RequestDelete) }
        render(ReminderEditorUiState::label, label::render)
        render(ReminderEditorUiState::enabled, enabled::render)
        render(ReminderEditorUiState::date) { binding.reminderEditorDate.text = ReminderFormat.dateLabel(it) }
        render({ it.hour to it.minute }) { (hour, minute) -> binding.reminderEditorTime.text = ReminderFormat.timeLabel(hour, minute) }
        render(ReminderEditorUiState::editing) { binding.reminderEditorDelete.setVisible(it) }
        render(ReminderEditorUiState::status, ::renderStatus)
    }

    private fun renderStatus(status: EditorStatus) = with(binding) {
        reminderEditorSave.isEnabled = status.idle
        reminderEditorDelete.isEnabled = status.idle
        reminderEditorProgress.setVisible(status.busy)
        reminderEditorError.setVisible(status.error != null)
        reminderEditorError.text = status.error?.userMessage(requireContext())
    }

    override fun onEvent(event: ReminderEditorEvent) {
        when (event) {
            is ReminderEditorEvent.PickDate -> childFragmentManager.showDatePicker(event.selected, LocalDate.now()) {
                dispatch(ReminderEditorIntent.SetDate(it))
            }
            is ReminderEditorEvent.PickTime -> childFragmentManager.showTimePicker(event.hour, event.minute) { h, m ->
                dispatch(ReminderEditorIntent.SetTime(h, m))
            }
            ReminderEditorEvent.ConfirmDelete -> confirmDelete()
            is ReminderEditorEvent.Completed -> complete(event.completion)
            is ReminderEditorEvent.OpenRoute -> navigate(event.route)
        }
    }

    private fun confirmDelete() {
        confirm(R.string.reminders_delete_confirm, DesignR.string.ds_action_delete, DesignR.string.ds_action_cancel) {
            dispatch(ReminderEditorIntent.Delete)
        }
    }

    private fun complete(completion: ReminderEditorCompletion) {
        val message = when (completion) {
            ReminderEditorCompletion.SAVED -> R.string.reminders_saved
            ReminderEditorCompletion.DELETED -> R.string.reminders_deleted
        }
        showMessage(message)
        dispatch(ReminderEditorIntent.CompletionShown)
    }
}
