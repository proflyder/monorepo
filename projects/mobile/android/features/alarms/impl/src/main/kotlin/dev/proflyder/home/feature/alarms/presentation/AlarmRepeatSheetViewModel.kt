package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.StateViewModel

internal data class AlarmRepeatSheetState(val choice: AlarmRepeatChoice) {
    val canConfirm: Boolean
        get() = choice.repeat != RepeatKind.CUSTOM || choice.days.isNotEmpty()
}

internal sealed interface AlarmRepeatIntent {
    data class Choose(val repeat: RepeatKind) : AlarmRepeatIntent
    data class SetDay(val day: Int, val selected: Boolean) : AlarmRepeatIntent
    data object Confirm : AlarmRepeatIntent
}

internal sealed interface AlarmRepeatEvent {
    data class Confirmed(val choice: AlarmRepeatChoice) : AlarmRepeatEvent
}

internal class AlarmRepeatSheetViewModel(
    private val savedState: SavedStateHandle,
) : StateViewModel<AlarmRepeatSheetState, AlarmRepeatIntent, AlarmRepeatEvent>(AlarmRepeatSheetState(restore(savedState))) {
    override fun handle(intent: AlarmRepeatIntent) {
        when (intent) {
            is AlarmRepeatIntent.Choose -> choose(intent.repeat)
            is AlarmRepeatIntent.SetDay -> setDay(intent.day, intent.selected)
            AlarmRepeatIntent.Confirm -> confirm()
        }
    }

    private fun choose(repeat: RepeatKind) {
        val days = when (repeat) {
            RepeatKind.CUSTOM -> currentState.choice.days
            else -> AlarmRepeatRules.daysForPreset(repeat).ifEmpty { currentState.choice.days }
        }
        change(AlarmRepeatChoice(repeat, days))
    }

    private fun setDay(day: Int, selected: Boolean) {
        val days = currentState.choice.days
        change(currentState.choice.copy(days = if (selected) days + day else days - day))
    }

    private fun confirm() {
        if (currentState.canConfirm) emit(AlarmRepeatEvent.Confirmed(currentState.choice.normalized()))
    }

    private fun change(choice: AlarmRepeatChoice) {
        updateState { copy(choice = choice) }
        savedState[KEY_REPEAT] = choice.repeat.name
        savedState[KEY_DAYS] = choice.days.sorted().toIntArray()
    }

    companion object {
        const val KEY_REPEAT = "alarm_repeat_choice"
        const val KEY_DAYS = "alarm_repeat_days"

        private fun restore(savedState: SavedStateHandle): AlarmRepeatChoice {
            val repeat = savedState.get<String>(KEY_REPEAT)
                ?.let { name -> RepeatKind.entries.firstOrNull { it.name == name } }
                ?: RepeatKind.ONCE
            val stored = savedState.get<IntArray>(KEY_DAYS)?.toSet().orEmpty()
            return AlarmRepeatChoice(repeat, stored.ifEmpty { AlarmRepeatRules.daysForPreset(repeat) })
        }
    }
}
