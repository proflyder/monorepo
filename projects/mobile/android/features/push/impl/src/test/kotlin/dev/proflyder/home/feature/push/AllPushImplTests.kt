package dev.proflyder.home.feature.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Payload contract with the server: `phone_find` is data-only and wins over a
 * notification block, ordinary notifications need a title and a body, and a
 * foreign deep link is dropped instead of trusted.
 */
class PushPayloadsTest {
    @Test
    fun aFindDataMessageIsDecodedWithItsReason() {
        val message = PushPayloads.decode(
            data = mapOf("type" to "phone_find", "reason" to "  Рикардо  "),
            notificationTitle = null,
            notificationBody = null,
        )
        assertEquals(PushMessage.Find("Рикардо"), message)
    }

    @Test
    fun aFindMessageWinsOverANotificationBlock() {
        val message = PushPayloads.decode(
            data = mapOf("type" to "phone_find"),
            notificationTitle = "ignored",
            notificationBody = "ignored",
        )
        assertEquals(PushMessage.Find(""), message)
    }

    @Test
    fun aNotificationCarriesTheDeepLink() {
        val message = PushPayloads.decode(
            data = mapOf("type" to "phone_notify", "deepLink" to "home://alarms"),
            notificationTitle = "Дом",
            notificationBody = "Дверь открыта",
        )
        assertEquals(
            PushMessage.Notify("Дом", "Дверь открыта", PushDeepLink("alarms")),
            message,
        )
    }

    @Test
    fun theDataBlockOverridesTheNotificationBlock() {
        val message = PushPayloads.decode(
            data = mapOf("title" to "Из данных", "body" to "Текст"),
            notificationTitle = "Из notification",
            notificationBody = "Другой текст",
        )
        assertEquals(PushMessage.Notify("Из данных", "Текст", null), message)
    }

    @Test
    fun foreignDeepLinksAndEmptyBodiesAreRejected() {
        val foreign = PushPayloads.decode(
            data = mapOf("deepLink" to "https://example.test/x"),
            notificationTitle = "Дом",
            notificationBody = "Текст",
        )
        assertEquals(PushMessage.Notify("Дом", "Текст", null), foreign)
        assertNull(PushPayloads.decode(emptyMap(), null, null))
        assertNull(PushPayloads.decode(emptyMap(), "Дом", " "))
    }

    @Test
    fun longTextIsCutToTheDisplayBound() {
        val message = PushPayloads.decode(
            data = mapOf("title" to "a".repeat(500), "body" to "b".repeat(900)),
            notificationTitle = null,
            notificationBody = null,
        ) as PushMessage.Notify
        assertEquals(PushPayloads.MAX_TITLE_CHARS, message.title.length)
        assertEquals(PushPayloads.MAX_BODY_CHARS, message.body.length)
    }
}

/** One ring stops on «Нашёл» or after a minute, never later. */
class FindPhoneTimingTest {
    @Test
    fun theRingLastsExactlyOneMinute() {
        assertEquals(60_000L, FindPhoneTiming.AUTO_STOP_MILLIS)
        assertEquals(60_000L, FindPhoneTiming.remainingMillis(0, 0))
        assertEquals(1L, FindPhoneTiming.remainingMillis(0, 59_999))
        assertEquals(0L, FindPhoneTiming.remainingMillis(0, 60_000))
        assertTrue(FindPhoneTiming.isExpired(1_000, 61_000))
    }

    @Test
    fun aClockGoingBackwardsNeverExtendsTheRing() {
        assertFalse(FindPhoneTiming.isExpired(10_000, 0))
        assertEquals(60_000L, FindPhoneTiming.remainingMillis(10_000, 0))
    }
}
