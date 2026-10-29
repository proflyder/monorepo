package dev.proflyder.home.feature.alarms.presentation

import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import com.google.android.material.chip.Chip
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmEditorBinding
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

class AlarmEditorFragment :
    BaseFragment<FragmentAlarmEditorBinding>(FragmentAlarmEditorBinding.Companion::inflate) {
    private val viewModel: AlarmEditorViewModel by viewModels {
        viewModelFactory(AlarmEditorViewModel::class.java)
    }

    private var rendering = false

    /** Day chips in ISO order: index 0 is Monday (day 1). */
    private val dayChips: List<Chip>
        get() = with(binding) { listOf(day1, day2, day3, day4, day5, day6, day7) }

    override fun onBindingCreated(binding: FragmentAlarmEditorBinding, savedInstanceState: Bundle?) {
        val alarmId = arguments?.getString(AlarmEditorDestination.ARG_ALARM_ID)
        viewModel.start(alarmId)
        binding.alarmEditorDelete.setVisible(alarmId != null)

        binding.alarmEditorLabel.doAfterTextChanged { if (!rendering) viewModel.setLabel(it?.toString().orEmpty()) }
        binding.alarmEditorTimezone.doAfterTextChanged {
            if (!rendering) viewModel.setTimezone(it?.toString().orEmpty())
        }
        binding.alarmEditorEnabled.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        binding.alarmEditorTime.setOnClickListener { pickTime() }
        binding.alarmEditorDate.setOnClickListener { pickDate() }
        binding.alarmEditorSave.setOnClickListener { viewModel.save() }
        binding.alarmEditorDelete.setOnClickListener { confirmDelete() }

        binding.alarmEditorRepeatGroup.setOnCheckedStateChangeListener { _, checkedIds ->
            if (rendering) return@setOnCheckedStateChangeListener
            if (checkedIds.isNotEmpty()) viewModel.setRepeat(repeatFor(checkedIds.first()))
        }
        dayChips.forEachIndexed { index, chip ->
            chip.setOnCheckedChangeListener { _, checked ->
                if (!rendering) viewModel.setWeekday(index + 1, checked)
            }
        }

        viewModel.state.observe { render(it) }
        viewModel.completion.observe(Lifecycle.State.RESUMED) { completion ->
            if (completion == null) return@observe
            viewModel.consumeCompletion()
            showMessage(
                when (completion) {
                    AlarmEditorCompletion.SAVED -> R.string.alarms_saved
                    AlarmEditorCompletion.DELETED -> R.string.alarms_deleted
                },
            )
            requireNavigator().back()
        }
    }

    private fun render(state: AlarmEditorUiState) = with(binding) {
        rendering = true
        if (alarmEditorLabel.text?.toString() != state.label) alarmEditorLabel.setText(state.label)
        if (alarmEditorTimezone.text?.toString() != state.timezone) alarmEditorTimezone.setText(state.timezone)
        alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", state.hour, state.minute)
        alarmEditorDate.setVisible(state.repeat == RepeatKind.ONCE)
        val date = state.onceDate ?: LocalDate.now(zoneOrSystem(state.timezone))
        alarmEditorDate.text = date.format(DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
        alarmEditorEnabled.isChecked = state.enabled
        alarmEditorRepeatGroup.check(repeatChipId(state.repeat))
        alarmEditorDaysLabel.setVisible(state.repeat == RepeatKind.CUSTOM)
        alarmEditorDaysGroup.setVisible(state.repeat == RepeatKind.CUSTOM)
        dayChips.forEachIndexed { index, chip -> chip.isChecked = state.weekdays.contains(index + 1) }
        val idle = !state.saving && !state.loading && !state.loadFailed
        alarmEditorSave.isEnabled = idle
        alarmEditorDelete.isEnabled = idle
        alarmEditorProgress.setVisible(state.saving || state.loading)
        alarmEditorError.setVisible(state.error != null)
        alarmEditorError.text = state.error?.userMessage(requireContext())
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
        confirm(R.string.alarms_delete_confirm, R.string.alarms_delete_yes, R.string.alarms_cancel) {
            viewModel.delete()
        }
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
}
