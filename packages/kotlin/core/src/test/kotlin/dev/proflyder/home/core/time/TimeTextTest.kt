package dev.proflyder.home.core.time

import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TimeTextTest {
    private fun time(raw: String): TimeText.TimeResult =
        (TimeText.parseTime(raw) as TimeText.ParseResult.Valid).value

    private fun problem(raw: String): TimeText.TimeProblem =
        (TimeText.parseTime(raw) as TimeText.ParseResult.Invalid).reason

    @Test
    fun parsesStrictWallClockTime() {
        assertEquals(TimeText.TimeResult(0, 0), time("00:00"))
        assertEquals(TimeText.TimeResult(7, 5), time("7:05"))
        assertEquals(TimeText.TimeResult(23, 59), time("23:59"))
        assertEquals("07:05", time("7:05").display)
        assertEquals(java.time.LocalTime.of(7, 5), time("7:05").asLocalTime)
    }

    @Test
    fun rejectsInvalidWallClockTime() {
        assertEquals(TimeText.TimeProblem.EMPTY, problem(""))
        assertEquals(TimeText.TimeProblem.MALFORMED, problem("7"))
        assertEquals(TimeText.TimeProblem.MALFORMED, problem("07:5"))
        assertEquals(TimeText.TimeProblem.MALFORMED, problem("aa:bb"))
        assertEquals(TimeText.TimeProblem.HOUR_OUT_OF_RANGE, problem("24:00"))
        assertEquals(TimeText.TimeProblem.MINUTE_OUT_OF_RANGE, problem("12:60"))
    }

    @Test
    fun parsesIsoDateAndRejectsPastDates() {
        val today = LocalDate.of(2026, 10, 29)
        val future = TimeText.parseDateInFuture("2026-10-30", today)
        assertTrue(future is TimeText.ParseResult.Valid)
        assertEquals(LocalDate.of(2026, 10, 30), (future as TimeText.ParseResult.Valid).value)

        val todayResult = TimeText.parseDateInFuture("2026-10-29", today)
        assertTrue(todayResult is TimeText.ParseResult.Valid)

        val past = TimeText.parseDateInFuture("2026-10-28", today)
        assertEquals(
            TimeText.TimeProblem.DATE_IN_PAST,
            (past as TimeText.ParseResult.Invalid).reason,
        )
    }

    @Test
    fun rejectsMalformedDates() {
        assertEquals(
            TimeText.TimeProblem.DATE_MALFORMED,
            (TimeText.parseDate("31-12-2026") as TimeText.ParseResult.Invalid).reason,
        )
        assertEquals(
            TimeText.TimeProblem.DATE_MALFORMED,
            (TimeText.parseDate("2026-02-30") as TimeText.ParseResult.Invalid).reason,
        )
    }

    @Test
    fun validatesIanaTimezones() {
        val zone = TimeText.parseZone("Asia/Almaty")
        assertTrue(zone is TimeText.ParseResult.Valid)
        assertEquals(ZoneId.of("Asia/Almaty"), (zone as TimeText.ParseResult.Valid).value)

        assertEquals(
            TimeText.TimeProblem.UNKNOWN_TIMEZONE,
            (TimeText.parseZone("Mars/Olympus") as TimeText.ParseResult.Invalid).reason,
        )
        assertEquals(
            TimeText.TimeProblem.UNKNOWN_TIMEZONE,
            (TimeText.parseZone("Asia Almaty") as TimeText.ParseResult.Invalid).reason,
        )
    }

    @Test
    fun weekdayHelpers() {
        assertEquals(listOf(1, 3, 5), Weekdays.parse("1,3,5"))
        assertEquals(listOf(1, 3, 5), Weekdays.parse("5 3 1 3"))
        assertEquals(null, Weekdays.parse(""))
        assertEquals(null, Weekdays.parse("0,1"))
        assertEquals(null, Weekdays.parse("1,8"))
        assertEquals(listOf(1, 2, 3), Weekdays.normalize(listOf(3, 1, 2, 1)))
        assertEquals("1,2,3", Weekdays.toIsoString(listOf(3, 2, 1)))
        assertEquals(Weekdays.MONDAY_TO_FRIDAY, Weekdays.normalize(Weekdays.MONDAY_TO_FRIDAY))
    }
}
