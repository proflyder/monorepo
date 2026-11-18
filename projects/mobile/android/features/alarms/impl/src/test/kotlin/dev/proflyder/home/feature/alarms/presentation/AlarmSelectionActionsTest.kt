package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The bulk bar only offers actions that would change something: "Turn off"
 * for an all-on selection, "Turn on" for an all-off one, and both when the
 * picked alarms disagree.
 */
class AlarmSelectionActionsTest {
    private val on = alarm("a", enabled = true)
    private val off = alarm("b", enabled = false)

    @Test
    fun allOnOffersOnlyDisable() {
        assertEquals(
            AlarmSelectionActions(canEnable = false, canDisable = true),
            alarmSelectionActions(listOf(on), setOf(on.id)),
        )
    }

    @Test
    fun allOffOffersOnlyEnable() {
        assertEquals(
            AlarmSelectionActions(canEnable = true, canDisable = false),
            alarmSelectionActions(listOf(off), setOf(off.id)),
        )
    }

    @Test
    fun mixedSelectionOffersBoth() {
        assertEquals(
            AlarmSelectionActions(canEnable = true, canDisable = true),
            alarmSelectionActions(listOf(on, off), setOf(on.id, off.id)),
        )
    }

    @Test
    fun unpickedAlarmsDoNotCount() {
        assertEquals(
            AlarmSelectionActions(canEnable = false, canDisable = true),
            alarmSelectionActions(listOf(on, off), setOf(on.id)),
        )
    }

    @Test
    fun emptySelectionOffersNothing() {
        assertEquals(
            AlarmSelectionActions(canEnable = false, canDisable = false),
            alarmSelectionActions(listOf(on, off), emptySet()),
        )
    }

    private fun alarm(id: String, enabled: Boolean): Schedule = Schedule(
        id = id,
        label = id,
        kind = ScheduleKind.ALARM,
        timezone = "UTC",
        at = WallTime(10, 0),
        onceDate = "",
        repeat = RepeatKind.WEEKDAYS,
        weekdays = listOf(1, 2, 3, 4, 5),
        nextDueUnix = 0,
        enabled = enabled,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
    )
}
