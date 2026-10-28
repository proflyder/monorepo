package dev.proflyder.home.feature.reminders.presentation

import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.os.Bundle
import android.view.View
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.button.MaterialButton
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.reminders.ReminderEditorCompletion
import dev.proflyder.home.feature.reminders.ReminderEditorDestination
import dev.proflyder.home.feature.reminders.impl.R
import java.time.LocalDate
import kotlinx.coroutines.launch

class ReminderEditorFragment : Fragment(R.layout.fragment_reminder_editor) {
    private val viewModel: ReminderEditorViewModel by viewModels {
        viewModelFactory(ReminderEditorViewModel::class.java)
    }

    private lateinit var labelInput: TextInputEditText
    private lateinit var timezoneInput: TextInputEditText
    private lateinit var dateButton: MaterialButton
    private lateinit var timeButton: MaterialButton
    private lateinit var enabledSwitch: MaterialSwitch
    private lateinit var saveButton: MaterialButton
    private lateinit var deleteButton: MaterialButton
    private lateinit var progress: View
    private lateinit var errorText: TextView
    private var rendering = false

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        labelInput = view.findViewById(R.id.reminder_editor_label)
        timezoneInput = view.findViewById(R.id.reminder_editor_timezone)
        dateButton = view.findViewById(R.id.reminder_editor_date)
        timeButton = view.findViewById(R.id.reminder_editor_time)
        enabledSwitch = view.findViewById(R.id.reminder_editor_enabled)
        saveButton = view.findViewById(R.id.reminder_editor_save)
        deleteButton = view.findViewById(R.id.reminder_editor_delete)
        progress = view.findViewById(R.id.reminder_editor_progress)
        errorText = view.findViewById(R.id.reminder_editor_error)

        val reminderId = arguments?.getString(ReminderEditorDestination.ARG_REMINDER_ID)
        viewModel.start(reminderId)
        deleteButton.visibility = if (reminderId == null) View.GONE else View.VISIBLE

        labelInput.doAfterTextChanged { if (!rendering) viewModel.setLabel(it?.toString().orEmpty()) }
        timezoneInput.doAfterTextChanged { if (!rendering) viewModel.setTimezone(it?.toString().orEmpty()) }
        enabledSwitch.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        dateButton.setOnClickListener { pickDate() }
        timeButton.setOnClickListener { pickTime() }
        saveButton.setOnClickListener { viewModel.save() }
        deleteButton.setOnClickListener { confirmDelete() }

        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.state.collect(::render)
            }
        }
        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.RESUMED) {
                viewModel.completion.collect { completion ->
                    if (completion == null) return@collect
                    viewModel.consumeCompletion()
                    Snackbar.make(
                        requireView(),
                        if (completion == ReminderEditorCompletion.DELETED) {
                            R.string.reminders_deleted
                        } else {
                            R.string.reminders_saved
                        },
                        Snackbar.LENGTH_SHORT,
                    ).show()
                    requireNavigator().back()
                }
            }
        }
    }

    private fun render(state: ReminderEditorUiState) {
        rendering = true
        if (labelInput.text?.toString() != state.label) labelInput.setText(state.label)
        if (timezoneInput.text?.toString() != state.timezone) timezoneInput.setText(state.timezone)
        dateButton.text = ReminderFormat.dateLabel(state.date)
        timeButton.text = ReminderFormat.timeLabel(state.hour, state.minute)
        enabledSwitch.isChecked = state.enabled
        saveButton.isEnabled = !state.saving && !state.loading && !state.loadFailed
        deleteButton.isEnabled = !state.saving && !state.loading && !state.loadFailed
        progress.visibility = if (state.saving || state.loading) View.VISIBLE else View.GONE
        errorText.visibility = if (state.error != null) View.VISIBLE else View.GONE
        errorText.text = state.error?.userMessage(requireContext())
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
        AlertDialog.Builder(requireContext())
            .setMessage(R.string.reminders_delete_confirm)
            .setNegativeButton(R.string.reminders_cancel, null)
            .setPositiveButton(R.string.reminders_delete_yes) { _, _ -> viewModel.delete() }
            .show()
    }
}
