package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.TimerState
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SchedulerJsonTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun parsesInt64FromStringsAndNumbers() {
        val timer = SchedulerJson.parseTimer(
            json.parseToJsonElement(
                """{"id":"t","durationSeconds":"3600","remainingSeconds":12,"dueUnix":"99",
                   "state":"TIMER_STATE_PAUSED","revision":"7"}""",
            ),
        )
        assertEquals(3600L, timer.durationSeconds)
        assertEquals(12L, timer.remainingSeconds)
        assertEquals(99L, timer.dueUnix)
        assertEquals(7L, timer.revision)
        assertEquals(TimerState.PAUSED, timer.state)
    }

    @Test
    fun unknownEnumsFallBackToUnspecified() {
        val schedule = SchedulerJson.parseSchedule(
            json.parseToJsonElement(
                """{"id":"s","kind":"SCHEDULE_KIND_FUTURE","repeat":"REPEAT_KIND_FUTURE"}""",
            ),
        )
        assertEquals(ScheduleKind.UNSPECIFIED, schedule.kind)
        assertEquals(RepeatKind.UNSPECIFIED, schedule.repeat)
    }

    @Test
    fun missingWallTimeParsesAsMidnightNotAsCrash() {
        val schedule = SchedulerJson.parseSchedule(json.parseToJsonElement("""{"id":"s"}"""))
        assertEquals(0, schedule.at.hour)
        assertEquals(0, schedule.at.minute)
        assertEquals("", schedule.label)
    }

    @Test
    fun parsesActiveOccurrenceFieldsFromAddendum() {
        val schedule = SchedulerJson.parseSchedule(
            json.parseToJsonElement(
                """{"id":"s","activeOccurrenceUnix":"111","activeUntilUnix":"222"}""",
            ),
        )
        assertEquals(111L, schedule.activeOccurrenceUnix)
        assertEquals(222L, schedule.activeUntilUnix)
    }

    @Test
    fun statusParsesQueueAndConsumerFieldsTolerantly() {
        val status = SchedulerJson.parseStatus(
            json.parseToJsonElement(
                """{"version":"dev","schemaVersion":"3","nowUnix":"1000","queues":[{"name":"home"}],
                   "consumers":[]}""",
            ),
        )
        assertEquals("dev", status.version)
        assertEquals(3, status.schemaVersion)
        assertEquals(1000L, status.nowUnix)
        assertTrue(status.consumers.isEmpty())
    }
}
