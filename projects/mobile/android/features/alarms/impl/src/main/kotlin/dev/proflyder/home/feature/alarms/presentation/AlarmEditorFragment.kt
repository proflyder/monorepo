package dev.proflyder.home.feature.alarms.presentation

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
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.snackbar.Snackbar
import com.google.android.material.textfield.TextInputEditText
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import java.time.LocalDate
import java.time.ZoneId
import java.util.Locale
import kotlinx.coroutines.launch

class AlarmEditorFragment : Fragment(R.layout.fragment_alarm_editor) {
    private val viewModel: AlarmEditorViewModel by viewModels {
        viewModelFactory(AlarmEditorViewModel::class.java)
    }

    private lateinit var labelInput: TextInputEditText
    private lateinit var timezoneInput: TextInputEditText
    private lateinit var timeButton: MaterialButton
    private lateinit var dateButton: MaterialButton
    private lateinit var repeatGroup: ChipGroup
    private lateinit var daysGroup: ChipGroup
    private lateinit var daysLabel: TextView
    private lateinit var enabledSwitch: MaterialSwitch
    private lateinit var saveButton: MaterialButton
    private lateinit var deleteButton: MaterialButton
    private lateinit var progress: View
    private lateinit var errorText: TextView

    private var rendering = false

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        labelInput = view.findViewById(R.id.alarm_editor_label)
        timezoneInput = view.findViewById(R.id.alarm_editor_timezone)
        timeButton = view.findViewById(R.id.alarm_editor_time)
        dateButton = view.findViewById(R.id.alarm_editor_date)
        repeatGroup = view.findViewById(R.id.alarm_editor_repeat_group)
        daysGroup = view.findViewById(R.id.alarm_editor_days_group)
        daysLabel = view.findViewById(R.id.alarm_editor_days_label)
        enabledSwitch = view.findViewById(R.id.alarm_editor_enabled)
        saveButton = view.findViewById(R.id.alarm_editor_save)
        deleteButton = view.findViewById(R.id.alarm_editor_delete)
        progress = view.findViewById(R.id.alarm_editor_progress)
        errorText = view.findViewById(R.id.alarm_editor_error)

        val alarmId = arguments?.getString(AlarmEditorDestination.ARG_ALARM_ID)
        viewModel.start(alarmId)
        deleteButton.visibility = if (alarmId == null) View.GONE else View.VISIBLE

        labelInput.doAfterTextChanged { if (!rendering) viewModel.setLabel(it?.toString().orEmpty()) }
        timezoneInput.doAfterTextChanged { if (!rendering) viewModel.setTimezone(it?.toString().orEmpty()) }
        enabledSwitch.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        timeButton.setOnClickListener { pickTime() }
        dateButton.setOnClickListener { pickDate() }
        saveButton.setOnClickListener { viewModel.save() }
        deleteButton.setOnClickListener { confirmDelete() }

        repeatGroup.setOnCheckedStateChangeListener { _, checkedIds ->
            if (rendering) return@setOnCheckedStateChangeListener
            if (checkedIds.isNotEmpty()) viewModel.setRepeat(repeatFor(checkedIds.first()))
        }
        for (day in 1..7) {
            view.findViewById<Chip>(dayId(day)).setOnCheckedChangeListener { _, checked ->
                if (!rendering) viewModel.setWeekday(day, checked)
            }
        }

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
                    val message = when (completion) {
                        AlarmEditorCompletion.SAVED -> R.string.alarms_saved
                        AlarmEditorCompletion.DELETED -> R.string.alarms_deleted
                    }
                    Snackbar.make(requireView(), message, Snackbar.LENGTH_SHORT).show()
                    requireNavigator().back()
                }
            }
        }
    }

    private fun render(state: AlarmEditorUiState) {
        rendering = true
        if (labelInput.text?.toString() != state.label) labelInput.setText(state.label)
        if (timezoneInput.text?.toString() != state.timezone) timezoneInput.setText(state.timezone)
        timeButton.text = String.format(Locale.ROOT, "%02d:%02d", state.hour, state.minute)
        dateButton.visibility = if (state.repeat == RepeatKind.ONCE) View.VISIBLE else View.GONE
        val date = state.onceDate ?: LocalDate.now(zoneOrSystem(state.timezone))
        dateButton.text = date.format(java.time.format.DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
        enabledSwitch.isChecked = state.enabled
        repeatGroup.check(repeatChipId(state.repeat))
        daysLabel.visibility = if (state.repeat == RepeatKind.CUSTOM) View.VISIBLE else View.GONE
        daysGroup.visibility = if (state.repeat == RepeatKind.CUSTOM) View.VISIBLE else View.GONE
        for (day in 1..7) {
            view?.findViewById<Chip>(dayId(day))?.isChecked = state.weekdays.contains(day)
        }
        saveButton.isEnabled = !state.saving && !state.loading && !state.loadFailed
        deleteButton.isEnabled = !state.saving && !state.loading && !state.loadFailed
        progress.visibility = if (state.saving || state.loading) View.VISIBLE else View.GONE
        errorText.visibility = if (state.error != null) View.VISIBLE else View.GONE
        errorText.text = state.error?.userMessage(requireContext())
        rendering = false
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
            .setMessage(R.string.alarms_delete_confirm)
            .setNegativeButton(R.string.alarms_cancel, null)
            .setPositiveButton(R.string.alarms_delete_yes) { _, _ -> viewModel.delete() }
            .show()
    }

    private fun pickDate() {
        val state = viewModel.state.value
        val date = state.onceDate ?: LocalDate.now(zoneOrSystem(state.timezone))
        DatePickerDialog(
            requireContext(),
            { _, year, month, day -> viewModel.setDate(LocalDate.of(year, month + 1, day)) },
            date.year,
            date.monthValue - 1,
            date.dayOfMonth,
        ).apply {
            datePicker.minDate = LocalDate.now(zoneOrSystem(state.timezone))
                .atStartOfDay(zoneOrSystem(state.timezone))
                .toInstant()
                .toEpochMilli()
        }.show()
    }

    private fun zoneOrSystem(timezone: String): ZoneId =
        runCatching { ZoneId.of(timezone) }.getOrDefault(ZoneId.systemDefault())

    private fun repeatFor(chipId: Int): RepeatKind = when (chipId) {
        R.id.repeat_once -> RepeatKind.ONCE
        R.id.repeat_weekdays -> RepeatKind.WEEKDAYS
        R.id.repeat_weekends -> RepeatKind.WEEKENDS
        R.id.repeat_custom -> RepeatKind.CUSTOM
        else -> RepeatKind.DAILY
    }

    private fun repeatChipId(repeat: RepeatKind): Int = when (repeat) {
        RepeatKind.ONCE -> R.id.repeat_once
        RepeatKind.WEEKDAYS -> R.id.repeat_weekdays
        RepeatKind.WEEKENDS -> R.id.repeat_weekends
        RepeatKind.CUSTOM -> R.id.repeat_custom
        else -> R.id.repeat_daily
    }

    private fun dayId(day: Int): Int = when (day) {
        1 -> R.id.day_1
        2 -> R.id.day_2
        3 -> R.id.day_3
        4 -> R.id.day_4
        5 -> R.id.day_5
        6 -> R.id.day_6
        else -> R.id.day_7
    }
}
