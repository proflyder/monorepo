package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import org.junit.Assert.assertEquals
import org.junit.Test

/** Day set <-> preset mapping: the editor and the sheet share these rules. */
class AlarmRepeatRulesTest {
    @Test
    fun weekdaysMapToTheWeekdaysPreset() {
        assertEquals(RepeatKind.WEEKDAYS, AlarmRepeatRules.presetFor(setOf(1, 2, 3, 4, 5)))
    }

    @Test
    fun weekendMapsToTheWeekendsPreset() {
        assertEquals(RepeatKind.WEEKENDS, AlarmRepeatRules.presetFor(setOf(6, 7)))
    }

    @Test
    fun allSevenDaysMapToDaily() {
        assertEquals(RepeatKind.DAILY, AlarmRepeatRules.presetFor(setOf(1, 2, 3, 4, 5, 6, 7)))
    }

    @Test
    fun anyOtherSetStaysCustom() {
        assertEquals(RepeatKind.CUSTOM, AlarmRepeatRules.presetFor(setOf(1, 3, 5)))
        assertEquals(RepeatKind.CUSTOM, AlarmRepeatRules.presetFor(setOf(2, 4)))
    }

    @Test
    fun emptySetStaysCustom() {
        assertEquals(RepeatKind.CUSTOM, AlarmRepeatRules.presetFor(emptySet()))
    }

    @Test
    fun presetsExposeTheirDays() {
        assertEquals(setOf(1, 2, 3, 4, 5), AlarmRepeatRules.daysForPreset(RepeatKind.WEEKDAYS))
        assertEquals(setOf(6, 7), AlarmRepeatRules.daysForPreset(RepeatKind.WEEKENDS))
        assertEquals(setOf(1, 2, 3, 4, 5, 6, 7), AlarmRepeatRules.daysForPreset(RepeatKind.DAILY))
        assertEquals(emptySet<Int>(), AlarmRepeatRules.daysForPreset(RepeatKind.ONCE))
        assertEquals(emptySet<Int>(), AlarmRepeatRules.daysForPreset(RepeatKind.CUSTOM))
    }

    @Test
    fun normalizingACustomChoiceCollapsesPresetDays() {
        val choice = AlarmRepeatChoice(RepeatKind.CUSTOM, setOf(6, 7))
        assertEquals(RepeatKind.WEEKENDS, choice.normalized().repeat)
    }

    @Test
    fun normalizingKeepsANonPresetCustomChoice() {
        val choice = AlarmRepeatChoice(RepeatKind.CUSTOM, setOf(1, 3, 5))
        assertEquals(RepeatKind.CUSTOM, choice.normalized().repeat)
    }
}
