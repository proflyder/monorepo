package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.presentation.bindSwitch
import dev.proflyder.home.core.presentation.bindText
import dev.proflyder.home.core.designsystem.R as DesignR
import android.os.Bundle
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.RouteNotice
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.showNotice
import dev.proflyder.home.core.presentation.EditorStatus
import dev.proflyder.home.core.ui.injectedViewModels
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.alarms.ALARM_QUICK_OFFSETS_MINUTES
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

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        onResult(AlarmRepeatChosen) { AlarmEditorIntent.SetRepeat(it.repeat, it.days) }
    }

    override fun onViewReady(binding: FragmentAlarmEditorBinding, savedInstanceState: Bundle?) {
        binding.alarmEditorSaveBar.pinAboveKeyboard(binding.alarmEditorScroll)
        val label = bindText(binding.alarmEditorLabel, AlarmEditorIntent::SetLabel)
        val enabled = bindSwitch(binding.alarmEditorEnabled, AlarmEditorIntent::SetEnabled)
        bindClicks(binding)
        render(AlarmEditorUiState::label, label::render)
        render(AlarmEditorUiState::enabled, enabled::render)
        render({ it.hour to it.minute }) { (hour, minute) -> renderTime(hour, minute) }
        render({ it.repeat to it.weekdays }) { (repeat, days) -> renderRepeat(repeat, days) }
        render(AlarmEditorUiState::onceDate, ::renderDate)
        render(AlarmEditorUiState::editing) { binding.alarmEditorDelete.setVisible(it) }
        render(AlarmEditorUiState::status, ::renderStatus)
    }

    private fun bindClicks(binding: FragmentAlarmEditorBinding) {
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
    }

    private fun renderTime(hour: Int, minute: Int) {
        binding.alarmEditorTime.text = String.format(Locale.ROOT, "%02d:%02d", hour, minute)
    }

    private fun renderRepeat(repeat: RepeatKind, days: Set<Int>) {
        binding.alarmEditorRepeatValue.text = AlarmFormat.repeatValue(requireContext(), repeat, days)
        binding.alarmEditorDateRow.setVisible(repeat == RepeatKind.ONCE)
    }

    private fun renderDate(onceDate: LocalDate?) {
        val date = onceDate ?: LocalDate.now()
        binding.alarmEditorDateValue.text = date.format(DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
    }

    private fun renderStatus(status: EditorStatus) = with(binding) {
        alarmEditorSave.isEnabled = status.idle
        alarmEditorDelete.isEnabled = status.idle
        alarmEditorProgress.setVisible(status.busy)
        alarmEditorError.setVisible(status.error != null)
        alarmEditorError.text = status.error?.userMessage(requireContext())
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
            is AlarmEditorEvent.Completed -> complete(event.notice)
            is AlarmEditorEvent.OpenRoute -> navigate(event.route)
        }
    }

    private fun openRepeatSheet(repeat: RepeatKind, days: Set<Int>) {
        if (childFragmentManager.findFragmentByTag(AlarmRepeatSheet.TAG) != null) return
        AlarmRepeatSheet.create(repeat, days).show(childFragmentManager, AlarmRepeatSheet.TAG)
    }

    private fun confirmDelete() {
        confirm(R.string.alarms_delete_confirm, DesignR.string.ds_action_delete, DesignR.string.ds_action_cancel) {
            dispatch(AlarmEditorIntent.Delete)
        }
    }

    private fun complete(notice: RouteNotice) {
        showNotice(notice)
        dispatch(AlarmEditorIntent.CompletionShown)
    }
}
