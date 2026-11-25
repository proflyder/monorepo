package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.WallTime
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AlarmCountdownTest {
    private val almaty = ZoneId.of("Asia/Almaty")
    private val berlin = ZoneId.of("Europe/Berlin")

    @Test
    fun splitsIntoHoursAndMinutesFlooringSeconds() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertEquals(AlarmCountdown(0, 7, 25), alarmCountdown(now, now.plusSeconds(7 * 3600 + 25 * 60 + 59)))
    }

    @Test
    fun underMinuteWhenLessThanSixtySeconds() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertTrue(alarmCountdown(now, now.plusSeconds(59)).underMinute)
        assertTrue(alarmCountdown(now, now.minusSeconds(5)).underMinute)
        assertFalse(alarmCountdown(now, now.plusSeconds(60)).underMinute)
    }

    @Test
    fun daysAppearBeyondOneDay() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertEquals(AlarmCountdown(2, 3, 0), alarmCountdown(now, now.plus(Duration.ofHours(51))))
    }

    @Test
    fun dailyFiresTodayBeforeTimeAndTomorrowAfter() {
        val draft = draft(7, 0, RepeatKind.DAILY)
        assertEquals(local("2026-11-26T07:00", almaty), draft.nextTrigger(local("2026-11-26T06:00", almaty), almaty))
        assertEquals(local("2026-11-27T07:00", almaty), draft.nextTrigger(local("2026-11-26T07:00", almaty), almaty))
    }

    @Test
    fun weekdaysSkipTheWeekend() {
        val friday = local("2026-11-27T08:00", almaty)
        assertEquals(local("2026-11-30T07:00", almaty), draft(7, 0, RepeatKind.WEEKDAYS).nextTrigger(friday, almaty))
    }

    @Test
    fun customDaysWrapAroundTheWeek() {
        val draft = draft(7, 0, RepeatKind.CUSTOM, listOf(4))
        val thursdayAfter = local("2026-11-26T09:00", almaty)
        assertEquals(local("2026-12-03T07:00", almaty), draft.nextTrigger(thursdayAfter, almaty))
    }

    @Test
    fun onceInThePastHasNoTrigger() {
        val draft = draft(7, 0, RepeatKind.ONCE).copy(onceDate = LocalDate.parse("2026-11-25"))
        assertNull(draft.nextTrigger(local("2026-11-26T06:00", almaty), almaty))
    }

    @Test
    fun disabledHasNoTrigger() {
        assertNull(draft(7, 0, RepeatKind.DAILY).copy(enabled = false).nextTrigger(Instant.EPOCH, almaty))
    }

    @Test
    fun springForwardGapMovesToTheValidHour() {
        val now = local("2026-03-29T00:00", berlin)
        val trigger = draft(2, 30, RepeatKind.DAILY).nextTrigger(now, berlin)
        assertEquals(Instant.parse("2026-03-29T01:30:00Z"), trigger)
        assertEquals(AlarmCountdown(0, 2, 30), alarmCountdown(now, checkNotNull(trigger)))
    }

    @Test
    fun fallBackCountsTheRepeatedHour() {
        val now = local("2026-10-25T00:00", berlin)
        val trigger = checkNotNull(draft(7, 0, RepeatKind.DAILY).nextTrigger(now, berlin))
        assertEquals(AlarmCountdown(0, 8, 0), alarmCountdown(now, trigger))
    }

    @Test
    fun scheduleUsesServerTriggerWhenFuture() {
        val now = local("2026-11-26T06:00", almaty)
        val server = now.plus(Duration.ofDays(1)).epochSecond
        assertEquals(Instant.ofEpochSecond(server), schedule(server).nextTrigger(now, almaty))
    }

    @Test
    fun scheduleFallsBackToLocalTriggerWhenServerIsStale() {
        val now = local("2026-11-26T06:00", almaty)
        assertEquals(local("2026-11-26T07:00", almaty), schedule(now.epochSecond - 60).nextTrigger(now, almaty))
    }

    @Test
    fun countdownWindowIsTheNextTwentyFourHours() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertTrue(now.plus(Duration.ofHours(24)).withinCountdownWindow(now))
        assertFalse(now.plus(Duration.ofHours(24)).plusSeconds(1).withinCountdownWindow(now))
        assertFalse(now.withinCountdownWindow(now))
    }

    private fun local(value: String, zone: ZoneId): Instant = LocalDateTime.parse(value).atZone(zone).toInstant()

    private fun draft(hour: Int, minute: Int, repeat: RepeatKind, days: List<Int> = emptyList()) = AlarmDraft(
        label = "",
        hour = hour,
        minute = minute,
        repeat = repeat,
        weekdays = days,
        enabled = true,
    )

    private fun schedule(nextDueUnix: Long) = Schedule(
        id = "a",
        label = "",
        kind = ScheduleKind.ALARM,
        at = WallTime(7, 0),
        onceDate = "",
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        nextDueUnix = nextDueUnix,
        enabled = true,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
    )
}
