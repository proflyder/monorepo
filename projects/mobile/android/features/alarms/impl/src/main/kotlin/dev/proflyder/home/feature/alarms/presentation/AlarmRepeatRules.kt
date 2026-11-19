package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind

/**
 * Day sets are ISO weekdays: 1 is Monday, 7 is Sunday. A custom selection
 * that covers exactly a preset is stored as that preset, so the editor and the
 * home server never show two different names for the same days.
 */
internal object AlarmRepeatRules {
    val WEEKDAYS: Set<Int> = setOf(1, 2, 3, 4, 5)
    val WEEKENDS: Set<Int> = setOf(6, 7)
    val EVERY_DAY: Set<Int> = setOf(1, 2, 3, 4, 5, 6, 7)

    /** The preset with exactly these days, or CUSTOM for any other non-empty set. */
    fun presetFor(days: Set<Int>): RepeatKind = when (days) {
        WEEKDAYS -> RepeatKind.WEEKDAYS
        WEEKENDS -> RepeatKind.WEEKENDS
        EVERY_DAY -> RepeatKind.DAILY
        else -> RepeatKind.CUSTOM
    }

    /** The days a preset stands for; empty for ONCE and for an explicit custom set. */
    fun daysForPreset(repeat: RepeatKind): Set<Int> = when (repeat) {
        RepeatKind.DAILY -> EVERY_DAY
        RepeatKind.WEEKDAYS -> WEEKDAYS
        RepeatKind.WEEKENDS -> WEEKENDS
        else -> emptySet()
    }
}

/** What the repeat sheet currently holds and returns on "Done". */
internal data class AlarmRepeatChoice(
    val repeat: RepeatKind,
    val days: Set<Int>,
)

/** Turns a chosen set of days back into the preset it represents, if any. */
internal fun AlarmRepeatChoice.normalized(): AlarmRepeatChoice = when (repeat) {
    RepeatKind.CUSTOM -> copy(repeat = AlarmRepeatRules.presetFor(days))
    else -> this
}
