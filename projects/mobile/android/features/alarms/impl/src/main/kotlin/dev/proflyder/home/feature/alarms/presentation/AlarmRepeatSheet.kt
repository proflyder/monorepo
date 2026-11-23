package dev.proflyder.home.feature.alarms.presentation

import android.os.Bundle
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.CheckableBinder
import dev.proflyder.home.core.presentation.RadioGroupBinder
import dev.proflyder.home.core.presentation.ScreenBottomSheet
import dev.proflyder.home.core.presentation.setResult
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.SheetAlarmRepeatBinding
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal class AlarmRepeatSheet :
    ScreenBottomSheet<SheetAlarmRepeatBinding, AlarmRepeatSheetState, AlarmRepeatIntent, AlarmRepeatEvent>(
        SheetAlarmRepeatBinding.Companion::inflate,
    ) {
    override val viewModel: AlarmRepeatSheetViewModel by screenViewModel()

    override fun onViewReady(binding: SheetAlarmRepeatBinding, savedInstanceState: Bundle?) {
        binding.sheetCancel.setOnClickListener { dismiss() }
        binding.sheetDone.setOnClickListener { dispatch(AlarmRepeatIntent.Confirm) }
        val options = RadioGroupBinder(binding.repeatOptions, ::optionId, ::repeatFor) { dispatch(AlarmRepeatIntent.Choose(it)) }
        val days = dayToggles(binding).mapIndexed { index, toggle ->
            CheckableBinder(toggle) { selected -> dispatch(AlarmRepeatIntent.SetDay(index + 1, selected)) }
        }
        render({ it.choice.repeat }) { repeat -> renderRepeat(options, repeat) }
        render({ it.choice.days }) { selected -> days.forEachIndexed { index, day -> day.render(index + 1 in selected) } }
        render(AlarmRepeatSheetState::canConfirm) { binding.sheetDone.isEnabled = it }
    }

    private fun renderRepeat(options: RadioGroupBinder<RepeatKind>, repeat: RepeatKind) {
        options.render(repeat)
        binding.repeatDaysGroup.setVisible(repeat == RepeatKind.CUSTOM)
    }

    override fun onEvent(event: AlarmRepeatEvent) {
        when (event) {
            is AlarmRepeatEvent.Confirmed -> publish(event.choice)
        }
    }

    private fun publish(choice: AlarmRepeatChoice) {
        setResult(AlarmRepeatChosen, choice)
        dismiss()
    }

    private fun dayToggles(binding: SheetAlarmRepeatBinding) = with(binding) {
        listOf(repeatDay1, repeatDay2, repeatDay3, repeatDay4, repeatDay5, repeatDay6, repeatDay7)
    }

    private fun repeatFor(checkedId: Int): RepeatKind = when (checkedId) {
        R.id.repeat_option_once -> RepeatKind.ONCE
        R.id.repeat_option_daily -> RepeatKind.DAILY
        R.id.repeat_option_weekdays -> RepeatKind.WEEKDAYS
        R.id.repeat_option_weekends -> RepeatKind.WEEKENDS
        else -> RepeatKind.CUSTOM
    }

    private fun optionId(repeat: RepeatKind): Int = when (repeat) {
        RepeatKind.ONCE -> R.id.repeat_option_once
        RepeatKind.DAILY -> R.id.repeat_option_daily
        RepeatKind.WEEKDAYS -> R.id.repeat_option_weekdays
        RepeatKind.WEEKENDS -> R.id.repeat_option_weekends
        else -> R.id.repeat_option_custom
    }

    companion object {
        const val TAG = "alarm_repeat_sheet"

        fun create(repeat: RepeatKind, days: Set<Int>): AlarmRepeatSheet = AlarmRepeatSheet().apply {
            arguments = Bundle().apply {
                putString(AlarmRepeatSheetViewModel.KEY_REPEAT, repeat.name)
                putIntArray(AlarmRepeatSheetViewModel.KEY_DAYS, days.sorted().toIntArray())
            }
        }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(AlarmRepeatSheetViewModel::class)
@Inject
class AlarmRepeatSheetViewModelSource : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel = AlarmRepeatSheetViewModel(savedState)
}
