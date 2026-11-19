package dev.proflyder.home.feature.alarms.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.RepeatKind

/**
 * Selection state of the repeat sheet. [SavedStateHandle] receives the
 * arguments as defaults and then keeps every user change, so a rotation (or a
 * process recreation) reopens the sheet with the same choice. The sheet itself
 * publishes the result only on "Done", through the Fragment Result API.
 */
internal class AlarmRepeatSheetViewModel(private val savedState: SavedStateHandle) : ViewModel() {
    var choice: AlarmRepeatChoice = restore()
        private set

    /** A custom selection needs at least one day; every preset is always valid. */
    val canConfirm: Boolean
        get() = when (choice.repeat) {
            RepeatKind.CUSTOM -> choice.days.isNotEmpty()
            else -> true
        }

    fun choose(repeat: RepeatKind) {
        val days = when (repeat) {
            RepeatKind.CUSTOM -> choice.days
            else -> AlarmRepeatRules.daysForPreset(repeat).ifEmpty { choice.days }
        }
        choice = AlarmRepeatChoice(repeat, days)
        persist()
    }

    fun setDay(day: Int, selected: Boolean) {
        val days = choice.days.toMutableSet()
        when (selected) {
            true -> days.add(day)
            false -> days.remove(day)
        }
        choice = choice.copy(days = days)
        persist()
    }

    private fun persist() {
        savedState[KEY_REPEAT] = choice.repeat.name
        savedState[KEY_DAYS] = choice.days.sorted().toIntArray()
    }

    private fun restore(): AlarmRepeatChoice {
        val repeat = savedState.get<String>(KEY_REPEAT)
            ?.let { name -> RepeatKind.entries.firstOrNull { it.name == name } }
            ?: RepeatKind.ONCE
        val stored = savedState.get<IntArray>(KEY_DAYS)?.toSet().orEmpty()
        val days = stored.ifEmpty { AlarmRepeatRules.daysForPreset(repeat) }
        return AlarmRepeatChoice(repeat, days)
    }

    companion object {
        const val KEY_REPEAT = "alarm_repeat_choice"
        const val KEY_DAYS = "alarm_repeat_days"
    }
}
