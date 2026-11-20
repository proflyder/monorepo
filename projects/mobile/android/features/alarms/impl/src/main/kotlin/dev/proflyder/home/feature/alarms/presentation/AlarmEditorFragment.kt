package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import com.google.android.material.chip.Chip
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.ALARM_QUICK_OFFSETS_MINUTES
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.AlarmEditorDestination
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmEditorBinding
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale

class AlarmEditorFragment :
    BaseFragment<FragmentAlarmEditorBinding>(FragmentAlarmEditorBinding.Companion::inflate) {
    private val viewModel: AlarmEditorViewModel by viewModels {
        viewModelFactory(AlarmEditorViewModel::class.java)
    }

    private var rendering = false

    /** Quick offset chips in the same order as [ALARM_QUICK_OFFSETS_MINUTES]. */
    private val quickOffsetChips: List<Chip>
        get() = with(binding) {
            listOf(quickOffset15, quickOffset30, quickOffset60, quickOffset90, quickOffset120)
        }

    override fun onBindingCreated(binding: FragmentAlarmEditorBinding, savedInstanceState: Bundle?) {
        binding.alarmEditorSaveBar.pinAboveKeyboard(binding.alarmEditorScroll)
        val alarmId = arguments?.getString(AlarmEditorDestination.ARG_ALARM_ID)
        viewModel.start(alarmId)
        binding.alarmEditorDelete.setVisible(alarmId != null)

        binding.alarmEditorLabel.doAfterTextChanged { if (!rendering) viewModel.setLabel(it?.toString().orEmpty()) }
        binding.alarmEditorEnabled.setOnCheckedChangeListener { _, checked ->
            if (!rendering) viewModel.setEnabled(checked)
        }
        binding.alarmEditorTime.setOnClickListener { pickTime() }
        quickOffsetChips.forEachIndexed { index, chip ->
            val minutes = ALARM_QUICK_OFFSETS_MINUTES.getOrNull(index) ?: return@forEachIndexed
            chip.setOnClickListener { viewModel.applyQuickOffset(minutes) }
        }
        binding.alarmEditorRepeatRow.setOnClickListener { openRepeatSheet() }
        binding.alarmEditorDateRow.setOnClickListener { pickDate() }
        binding.alarmEditorSave.setOnClickListener { viewModel.save() }
        binding.alarmEditorDelete.setOnClickListener { confirmDelete() }
        childFragmentManager.setFragmentResultListener(
            AlarmRepeatSheet.REQUEST_KEY,
            viewLifecycleOwner,
        ) { _, result -> applyRepeatResult(result) }

        viewModel.state.observe { render(it) }
        viewModel.completion.observe(Lifecycle.State.RESUMED) { completion -> handleCompletion(completion) }
    }

    private fun handleCompletion(completion: AlarmEditorCompletion?) {
        if (completion == null) return
        viewModel.consumeCompletion()
        showMessage(
            when (completion) {
                AlarmEditorCompletion.SAVED -> R.string.alarms_saved
                AlarmEditorCompletion.DELETED -> R.string.alarms_deleted
            },
        )
        requireNavigator().back()
    }

    private fun render(state: AlarmEditorUiState) = with(binding) {
        rendering = true
        if (alarmEditorLabel.text?.toString() != state.label) alarmEditorLabel.setText(state.label)
        alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", state.hour, state.minute)
        alarmEditorRepeatValue.text = AlarmFormat.repeatValue(requireContext(), state.repeat, state.weekdays)
        alarmEditorDateRow.setVisible(state.repeat == RepeatKind.ONCE)
        val date = state.onceDate ?: LocalDate.now()
        alarmEditorDateValue.text = date.format(DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
        alarmEditorEnabled.isChecked = state.enabled
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
        childFragmentManager.showTimePicker(state.hour, state.minute, viewModel::setTime)
    }

    private fun confirmDelete() {
        confirm(R.string.alarms_delete_confirm, R.string.alarms_delete_yes, R.string.alarms_cancel) {
            viewModel.delete()
        }
    }

    private fun pickDate() {
        val state = viewModel.state.value
        val today = LocalDate.now()
        childFragmentManager.showDatePicker(state.onceDate ?: today, today, viewModel::setDate)
    }

    private fun openRepeatSheet() {
        val state = viewModel.state.value
        when (childFragmentManager.findFragmentByTag(AlarmRepeatSheet.TAG) != null) {
            true -> Unit
            false -> AlarmRepeatSheet.create(state.repeat, state.weekdays)
                .show(childFragmentManager, AlarmRepeatSheet.TAG)
        }
    }

    private fun applyRepeatResult(result: Bundle) {
        val repeat = RepeatKind.entries
            .firstOrNull { it.name == result.getString(AlarmRepeatSheet.RESULT_REPEAT) }
            ?: return
        val days = result.getIntArray(AlarmRepeatSheet.RESULT_DAYS)?.toSet().orEmpty()
        viewModel.setRepeatAndDays(repeat, days)
    }
}
