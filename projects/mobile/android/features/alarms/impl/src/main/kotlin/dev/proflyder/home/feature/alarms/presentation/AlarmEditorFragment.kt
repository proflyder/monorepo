package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.ui.injectedViewModels
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.alarms.ALARM_QUICK_OFFSETS_MINUTES
import dev.proflyder.home.feature.alarms.AlarmEditorCompletion
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmEditorBinding
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale

internal class AlarmEditorFragment :
    ScreenFragment<FragmentAlarmEditorBinding, AlarmEditorUiState, AlarmEditorIntent, AlarmEditorEvent>(
        FragmentAlarmEditorBinding.Companion::inflate,
    ) {
    override val viewModel: AlarmEditorViewModel by injectedViewModels()

    private var label: TextBinder? = null
    private var enabled: SwitchBinder? = null

    override fun onScreenCreated(binding: FragmentAlarmEditorBinding, savedInstanceState: Bundle?) {
        binding.alarmEditorSaveBar.pinAboveKeyboard(binding.alarmEditorScroll)
        label = bindText(binding.alarmEditorLabel, AlarmEditorIntent::SetLabel)
        enabled = bindSwitch(binding.alarmEditorEnabled, AlarmEditorIntent::SetEnabled)
        binding.alarmEditorTime.setOnClickListener { dispatch(AlarmEditorIntent.PickTime) }
        val chips = listOf(
            binding.quickOffset15,
            binding.quickOffset30,
            binding.quickOffset60,
            binding.quickOffset90,
            binding.quickOffset120,
        )
        chips.zip(ALARM_QUICK_OFFSETS_MINUTES).forEach { (chip, minutes) ->
            chip.setOnClickListener { dispatch(AlarmEditorIntent.QuickOffset(minutes)) }
        }
        binding.alarmEditorRepeatRow.setOnClickListener { dispatch(AlarmEditorIntent.PickRepeat) }
        binding.alarmEditorDateRow.setOnClickListener { dispatch(AlarmEditorIntent.PickDate) }
        binding.alarmEditorSave.setOnClickListener { dispatch(AlarmEditorIntent.Save) }
        binding.alarmEditorDelete.setOnClickListener { dispatch(AlarmEditorIntent.RequestDelete) }
        childFragmentManager.setFragmentResultListener(AlarmRepeatSheet.REQUEST_KEY, viewLifecycleOwner) { _, result ->
            AlarmRepeatSheet.resultOf(result)?.let(::dispatch)
        }
    }

    override fun onBindingDestroyed() {
        label = null
        enabled = null
    }

    override fun render(state: AlarmEditorUiState) = with(binding) {
        label?.render(state.label)
        enabled?.render(state.enabled)
        alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", state.hour, state.minute)
        alarmEditorRepeatValue.text = AlarmFormat.repeatValue(requireContext(), state.repeat, state.weekdays)
        alarmEditorDateRow.setVisible(state.repeat == RepeatKind.ONCE)
        val date = state.onceDate ?: LocalDate.now()
        alarmEditorDateValue.text = date.format(DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
        alarmEditorDelete.setVisible(state.editing)
        alarmEditorSave.isEnabled = state.status.idle
        alarmEditorDelete.isEnabled = state.status.idle
        alarmEditorProgress.setVisible(state.status.busy)
        alarmEditorError.setVisible(state.status.error != null)
        alarmEditorError.text = state.status.error?.userMessage(requireContext())
    }

    override fun onEvent(event: AlarmEditorEvent) {
        when (event) {
            is AlarmEditorEvent.PickTime -> childFragmentManager.showTimePicker(event.hour, event.minute) { h, m ->
                dispatch(AlarmEditorIntent.SetTime(h, m))
            }
            is AlarmEditorEvent.PickDate -> childFragmentManager.showDatePicker(event.selected, LocalDate.now()) {
                dispatch(AlarmEditorIntent.SetDate(it))
            }
            is AlarmEditorEvent.PickRepeat -> openRepeatSheet(event.repeat, event.days)
            AlarmEditorEvent.ConfirmDelete -> confirmDelete()
            is AlarmEditorEvent.Completed -> complete(event.completion)
            is AlarmEditorEvent.OpenRoute -> navigate(event.route)
        }
    }

    private fun openRepeatSheet(repeat: RepeatKind, days: Set<Int>) {
        if (childFragmentManager.findFragmentByTag(AlarmRepeatSheet.TAG) != null) return
        AlarmRepeatSheet.create(repeat, days).show(childFragmentManager, AlarmRepeatSheet.TAG)
    }

    private fun confirmDelete() {
        confirm(R.string.alarms_delete_confirm, R.string.alarms_delete_yes, R.string.alarms_cancel) {
            dispatch(AlarmEditorIntent.Delete)
        }
    }

    private fun complete(completion: AlarmEditorCompletion) {
        val message = when (completion) {
            AlarmEditorCompletion.SAVED -> R.string.alarms_saved
            AlarmEditorCompletion.DELETED -> R.string.alarms_deleted
        }
        showMessage(message)
        dispatch(AlarmEditorIntent.CompletionShown)
    }
}
