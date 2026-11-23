package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import org.junit.Assert.assertEquals
import org.junit.Test

class UpcomingAlarmRulesTest {
    private val now = 1_000L

    @Test
    fun aRingingAlarmLeadsAndDisabledOrUnscheduledAlarmsAreHidden() {
        val alarms = listOf(
            schedule("next").copy(nextDueUnix = now + 60),
            schedule("later").copy(nextDueUnix = now + 600),
            schedule("ringing").copy(nextDueUnix = now + 86_400, activeOccurrenceUnix = now - 10, activeUntilUnix = now + 60),
            schedule("off", enabled = false).copy(nextDueUnix = now + 30),
            schedule("unscheduled"),
        )
        val upcoming = UpcomingAlarmRules.upcoming(alarms, now)
        assertEquals(listOf("ringing", "next", "later"), upcoming.map { it.alarm.id })
        assertEquals(listOf(true, false, false), upcoming.map { it.ringing })
    }

    @Test
    fun atMostThreeAlarmsAreShown() {
        val alarms = (1..5).map { schedule("a$it").copy(nextDueUnix = now + it) }
        assertEquals(UpcomingAlarmRules.MAX_ROWS, UpcomingAlarmRules.upcoming(alarms, now).size)
    }
}
