package dev.proflyder.home.feature.reminders.presentation

import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.ui.applyScrollBottomInset
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

    private var label: TextBinder? = null
    private var enabled: SwitchBinder? = null

    override fun onScreenCreated(binding: FragmentReminderEditorBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        binding.root.keepFocusedVisible()
        label = bindText(binding.reminderEditorLabel, ReminderEditorIntent::SetLabel)
        enabled = bindSwitch(binding.reminderEditorEnabled, ReminderEditorIntent::SetEnabled)
        binding.reminderEditorDate.setOnClickListener { dispatch(ReminderEditorIntent.PickDate) }
        binding.reminderEditorTime.setOnClickListener { dispatch(ReminderEditorIntent.PickTime) }
        binding.reminderEditorSave.setOnClickListener { dispatch(ReminderEditorIntent.Save) }
        binding.reminderEditorDelete.setOnClickListener { dispatch(ReminderEditorIntent.RequestDelete) }
    }

    override fun onBindingDestroyed() {
        label = null
        enabled = null
    }

    override fun render(state: ReminderEditorUiState) = with(binding) {
        label?.render(state.label)
        enabled?.render(state.enabled)
        reminderEditorDate.text = ReminderFormat.dateLabel(state.date)
        reminderEditorTime.text = ReminderFormat.timeLabel(state.hour, state.minute)
        reminderEditorDelete.setVisible(state.editing)
        reminderEditorSave.isEnabled = state.status.idle
        reminderEditorDelete.isEnabled = state.status.idle
        reminderEditorProgress.setVisible(state.status.busy)
        reminderEditorError.setVisible(state.status.error != null)
        reminderEditorError.text = state.status.error?.userMessage(requireContext())
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
