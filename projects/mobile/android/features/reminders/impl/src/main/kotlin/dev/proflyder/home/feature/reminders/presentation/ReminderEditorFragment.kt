package dev.proflyder.home.feature.reminders.presentation

import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.impl.databinding.FragmentReminderEditorBinding
import java.time.LocalDate

class ReminderEditorFragment :
    BaseFragment<FragmentReminderEditorBinding>(FragmentReminderEditorBinding.Companion::inflate) {
    private val viewModel: ReminderEditorViewModel by viewModels {
        viewModelFactory(ReminderEditorViewModel::class.java)
    }

    private var rendering = false

    override fun onBindingCreated(binding: FragmentReminderEditorBinding, savedInstanceState: Bundle?) {
        val reminderId = arguments?.getString(ReminderEditorDestination.ARG_REMINDER_ID)
        viewModel.start(reminderId)
        binding.reminderEditorDelete.setVisible(reminderId != null)

        binding.reminderEditorLabel.doAfterTextChanged {
            if (!rendering) viewModel.setLabel(it?.toString().orEmpty())
        }
        binding.reminderEditorTimezone.doAfterTextChanged {
            if (!rendering) viewModel.setTimezone(it?.toString().orEmpty())
        }
        binding.reminderEditorEnabled.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        binding.reminderEditorDate.setOnClickListener { pickDate() }
        binding.reminderEditorTime.setOnClickListener { pickTime() }
        binding.reminderEditorSave.setOnClickListener { viewModel.save() }
        binding.reminderEditorDelete.setOnClickListener { confirmDelete() }

        viewModel.state.observe { render(it) }
        viewModel.completion.observe(Lifecycle.State.RESUMED) { completion ->
            if (completion == null) return@observe
            viewModel.consumeCompletion()
            showMessage(
                if (completion == ReminderEditorCompletion.DELETED) {
                    R.string.reminders_deleted
                } else {
                    R.string.reminders_saved
                },
            )
            requireNavigator().back()
        }
    }

    private fun render(state: ReminderEditorUiState) = with(binding) {
        rendering = true
        if (reminderEditorLabel.text?.toString() != state.label) reminderEditorLabel.setText(state.label)
        if (reminderEditorTimezone.text?.toString() != state.timezone) reminderEditorTimezone.setText(state.timezone)
        reminderEditorDate.text = ReminderFormat.dateLabel(state.date)
        reminderEditorTime.text = ReminderFormat.timeLabel(state.hour, state.minute)
        reminderEditorEnabled.isChecked = state.enabled
        val idle = !state.saving && !state.loading && !state.loadFailed
        reminderEditorSave.isEnabled = idle
        reminderEditorDelete.isEnabled = idle
        reminderEditorProgress.setVisible(state.saving || state.loading)
        reminderEditorError.setVisible(state.error != null)
        reminderEditorError.text = state.error?.userMessage(requireContext())
        rendering = false
    }

    private fun pickDate() {
        val current = viewModel.state.value.date
        DatePickerDialog(
            requireContext(),
            { _, year, month, day -> viewModel.setDate(LocalDate.of(year, month + 1, day)) },
            current.year,
            current.monthValue - 1,
            current.dayOfMonth,
        ).apply {
            datePicker.minDate = LocalDate.now().atStartOfDay(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()
        }.show()
    }

    private fun pickTime() {
        val state = viewModel.state.value
        TimePickerDialog(
            requireContext(),
            { _, hour, minute -> viewModel.setTime(hour, minute) },
            state.hour,
            state.minute,
            true,
        ).show()
    }

    private fun confirmDelete() {
        confirm(R.string.reminders_delete_confirm, R.string.reminders_delete_yes, R.string.reminders_cancel) {
            viewModel.delete()
        }
    }
}
