package dev.proflyder.home.core.time

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DurationTextTest {
    @Test
    fun parsesUnitForms() {
        assertEquals(90L, DurationText.parseOrNull("90s"))
        assertEquals(300L, DurationText.parseOrNull("5m"))
        assertEquals(7200L, DurationText.parseOrNull("2h"))
        assertEquals(5400L, DurationText.parseOrNull("1h 30m"))
        assertEquals(5400L, DurationText.parseOrNull("1h30m"))
        assertEquals(135L, DurationText.parseOrNull("2m 15s"))
        assertEquals(3723L, DurationText.parseOrNull("1h 2m 3s"))
    }

    @Test
    fun parsesClockForms() {
        assertEquals(90L, DurationText.parseOrNull("01:30"))
        assertEquals(3723L, DurationText.parseOrNull("1:02:03"))
        assertEquals(59L, DurationText.parseOrNull("00:59"))
    }

    @Test
    fun acceptsContractBoundaries() {
        assertEquals(1L, DurationText.parseOrNull("1s"))
        assertEquals(DurationText.MAX_SECONDS, DurationText.parseOrNull("168h"))
        assertEquals(DurationText.MAX_SECONDS, DurationText.parseOrNull("604800s"))
    }

    @Test
    fun rejectsOutOfRange() {
        assertEquals(
            DurationText.Result.OutOfRange(0),
            DurationText.parse("0s"),
        )
        assertEquals(
            DurationText.Result.OutOfRange(0),
            DurationText.parse("00:00"),
        )
        assertEquals(
            DurationText.Result.OutOfRange(DurationText.MAX_SECONDS + 1),
            DurationText.parse("168h 1s"),
        )
    }

    @Test
    fun rejectsMalformedInput() {
        assertNull(DurationText.parseOrNull(""))
        assertNull(DurationText.parseOrNull("5"))
        assertNull(DurationText.parseOrNull("abc"))
        assertNull(DurationText.parseOrNull("1:60"))
        assertNull(DurationText.parseOrNull("1:00:60"))
        assertEquals(7200L, DurationText.parseOrNull("1h 60m"))
        assertNull(DurationText.parseOrNull("1h -2m"))
        assertNull(DurationText.parseOrNull("1.5h"))
        assertNull(DurationText.parseOrNull("1h 2x"))
    }

    @Test
    fun formatsCountdown() {
        assertEquals("00:00", DurationText.format(0))
        assertEquals("00:09", DurationText.format(9))
        assertEquals("01:30", DurationText.format(90))
        assertEquals("1:02:03", DurationText.format(3723))
        assertEquals("00:00", DurationText.format(-5))
    }

    @Test
    fun describesDurations() {
        assertEquals("1 h 30 min", DurationText.describe(5400))
        assertEquals("45 s", DurationText.describe(45))
        assertEquals("0 s", DurationText.describe(0))
    }
}
