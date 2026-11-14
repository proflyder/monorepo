package dev.proflyder.home.feature.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The history keeps the newest messages, replaces a repeated id and stays bounded. */
class NotificationHistoryRulesTest {
    @Test
    fun aNewMessageGoesToTheTop() {
        val older = entry("a", 1_000)
        val newer = entry("b", 2_000)

        val log = NotificationHistoryRules.record(listOf(older), newer)

        assertEquals(listOf("b", "a"), log.map { it.id })
    }

    @Test
    fun theSameMessageIsNotStoredTwice() {
        val message = entry("a", 1_000)

        val log = NotificationHistoryRules.record(listOf(message), message)

        assertEquals(1, log.size)
        assertEquals("a", log.single().id)
    }

    @Test
    fun onlyTheNewestTwoHundredMessagesSurvive() {
        val full = (1..NotificationHistoryRules.MAX_ENTRIES).map { entry("old-$it", it.toLong()) }

        val log = NotificationHistoryRules.record(full, entry("newest", 9_000))

        assertEquals(NotificationHistoryRules.MAX_ENTRIES, log.size)
        assertEquals("newest", log.first().id)
        assertTrue("the oldest message is dropped", log.none { it.id == "old-${NotificationHistoryRules.MAX_ENTRIES}" })
    }

    @Test
    fun anOverlongStoredLogIsTrimmedOnRead() {
        val overlong = (1..250).map { entry("e-$it", it.toLong()) }

        assertEquals(NotificationHistoryRules.MAX_ENTRIES, NotificationHistoryRules.bounded(overlong).size)
    }

    private fun entry(id: String, millis: Long): NotificationHistoryEntry = NotificationHistoryEntry(
        id = id,
        type = NotificationHistoryType.NOTIFY,
        title = "title",
        body = "body",
        receivedAtMillis = millis,
    )
}
