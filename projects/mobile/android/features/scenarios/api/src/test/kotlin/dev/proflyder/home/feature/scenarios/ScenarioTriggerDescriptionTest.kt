package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Test

class ScenarioTriggerDescriptionTest {
    private val ru = object : ScenarioTriggerStrings {
        override fun manual() = "Вручную"
        override fun daily() = "Ежедневно"
        override fun weekdays() = "Будни"
        override fun weekends() = "Выходные"
        override fun once(date: LocalDate?) = if (date == null) "Однократно" else "Однократно $date"
        override fun custom(days: List<Int>) = days.sorted().joinToString(", ") { "Д$it" }
        override fun unsupported() = "Не поддерживается"
        override fun time(hour: Int, minute: Int) = "%02d:%02d".format(hour, minute)
    }

    private fun time(
        hour: Int,
        minute: Int,
        repeat: RepeatKind,
        weekdays: List<Int> = emptyList(),
        onceDate: String = "",
    ) = ScenarioTrigger.Time(TimeTrigger("Asia/Almaty", WallTime(hour, minute), repeat, onceDate, weekdays))

    @Test
    fun weekdaysReadAsTheCardSummary() {
        assertEquals(
            "Будни 07:30",
            describeTriggers(listOf(time(7, 30, RepeatKind.WEEKDAYS)), ru),
        )
    }

    @Test
    fun manualIsImplicitAndOnlyShownAlone() {
        assertEquals("Вручную", describeTriggers(emptyList(), ru))
        assertEquals("Вручную", describeTriggers(listOf(ScenarioTrigger.Manual), ru))
        assertEquals(
            "Ежедневно 08:00",
            describeTriggers(listOf(ScenarioTrigger.Manual, time(8, 0, RepeatKind.DAILY)), ru),
        )
    }

    @Test
    fun everyRepeatKindHasASummary() {
        assertEquals("Выходные 10:15", describeTriggers(listOf(time(10, 15, RepeatKind.WEEKENDS)), ru))
        assertEquals(
            "Однократно 2026-11-09 09:00",
            describeTriggers(listOf(time(9, 0, RepeatKind.ONCE, onceDate = "2026-11-09")), ru),
        )
        assertEquals(
            "Д1, Д3 06:45",
            describeTriggers(listOf(time(6, 45, RepeatKind.CUSTOM, weekdays = listOf(3, 1))), ru),
        )
    }

    @Test
    fun severalTriggersJoinAndUnknownIsReported() {
        assertEquals(
            "Ежедневно 08:00, Выходные 10:00, Не поддерживается",
            describeTriggers(
                listOf(
                    time(8, 0, RepeatKind.DAILY),
                    time(10, 0, RepeatKind.WEEKENDS),
                    ScenarioTrigger.Unknown("""{"future":{}}"""),
                ),
                ru,
            ),
        )
    }
}
