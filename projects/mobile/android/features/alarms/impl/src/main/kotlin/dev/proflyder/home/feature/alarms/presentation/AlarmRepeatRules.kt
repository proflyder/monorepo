package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.presentation.ResultKey
import kotlinx.serialization.Serializable

internal object AlarmRepeatRules {
    val WEEKDAYS: Set<Int> = setOf(1, 2, 3, 4, 5)
    val WEEKENDS: Set<Int> = setOf(6, 7)
    val EVERY_DAY: Set<Int> = setOf(1, 2, 3, 4, 5, 6, 7)

    fun presetFor(days: Set<Int>): RepeatKind = when (days) {
        WEEKDAYS -> RepeatKind.WEEKDAYS
        WEEKENDS -> RepeatKind.WEEKENDS
        EVERY_DAY -> RepeatKind.DAILY
        else -> RepeatKind.CUSTOM
    }

    fun daysForPreset(repeat: RepeatKind): Set<Int> = when (repeat) {
        RepeatKind.DAILY -> EVERY_DAY
        RepeatKind.WEEKDAYS -> WEEKDAYS
        RepeatKind.WEEKENDS -> WEEKENDS
        else -> emptySet()
    }
}

@Serializable
internal data class AlarmRepeatChoice(
    val repeat: RepeatKind,
    val days: Set<Int>,
)

internal fun AlarmRepeatChoice.normalized(): AlarmRepeatChoice = when (repeat) {
    RepeatKind.CUSTOM -> copy(repeat = AlarmRepeatRules.presetFor(days))
    else -> this
}

internal object AlarmRepeatChosen : ResultKey<AlarmRepeatChoice>(AlarmRepeatChoice.serializer())
