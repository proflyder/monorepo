package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.feature.alarms.AlarmCountdown
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-rRU")
class AlarmCountdownTextTest {
    private val resources get() = RuntimeEnvironment.getApplication().resources

    @Test
    fun hoursAndMinutes() = assertEquals("через 7 ч 25 мин", text(0, 7, 25))

    @Test
    fun underMinute() = assertEquals("меньше чем через минуту", text(0, 0, 0))

    @Test
    fun zeroPartsAreDropped() {
        assertEquals("через 3 ч", text(0, 3, 0))
        assertEquals("через 1 мин", text(0, 0, 1))
    }

    @Test
    fun dayPlurals() {
        assertEquals("через 1 день 2 ч", text(1, 2, 0))
        assertEquals("через 2 дня 5 мин", text(2, 0, 5))
        assertEquals("через 5 дней", text(5, 0, 0))
        assertEquals("через 21 день", text(21, 0, 0))
    }

    @Test
    fun noticeWrapsTheCountdown() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertEquals(
            "Будильник сработает через 7 ч 25 мин",
            AlarmCountdownText.notice(resources, now, now.plusSeconds(7 * 3600 + 25 * 60 + 30)),
        )
    }

    @Test
    @Config(qualifiers = "en")
    fun englishNotice() {
        val now = Instant.parse("2026-11-26T00:00:00Z")
        assertEquals("Alarm will ring in less than a minute", AlarmCountdownText.notice(resources, now, now.plusSeconds(30)))
    }

    private fun text(days: Long, hours: Long, minutes: Long) =
        AlarmCountdownText.relative(resources, AlarmCountdown(days, hours, minutes))
}
