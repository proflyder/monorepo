package dev.proflyder.home.feature.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Suite

/** All push API tests, run as one Bazel target. */
@RunWith(Suite::class)
@Suite.SuiteClasses(
    PushApiTest::class,
    NotificationHistoryRulesTest::class,
)
class AllPushApiTests

class PushApiTest {
    @Test
    fun aHomeDeepLinkIsParsedIntoItsRoute() {
        assertEquals("alarms", PushDeepLink.parse("home://alarms")?.route)
        assertEquals("time/alarms", PushDeepLink.parse("  home://time/alarms/  ")?.route)
    }

    @Test
    fun foreignOrUnsafeDeepLinksAreRejected() {
        assertNull(PushDeepLink.parse(null))
        assertNull(PushDeepLink.parse(""))
        assertNull(PushDeepLink.parse("https://example.test/alarms"))
        assertNull(PushDeepLink.parse("home://"))
        assertNull(PushDeepLink.parse("home://alarms?x=<script>"))
        assertNull(PushDeepLink.parse("home://" + "a".repeat(300)))
    }

    @Test
    fun theRingOnlyPromisesWhatEveryPermissionAllows() {
        val none = PushPermissionStatus()
        assertFalse(none.canRing)
        assertTrue(PushPermissionStatus(notifications = true, dndAccess = true, fullScreenIntent = true).canRing)
        assertFalse(PushPermissionStatus(notifications = true, dndAccess = true).canRing)
        assertFalse(PushPermissionStatus(notifications = true, fullScreenIntent = true).canRing)
    }

    @Test
    fun theRegistrationShowsOnlyTheTokenTail() {
        // Built from parts so the literal can never look like a real credential
        // to a secret scanner.
        val token = listOf("fcm", "registration", "tail123456").joinToString("-")
        val registration = PushRegistration(token = token)
        assertEquals("123456", registration.tokenHint())
        assertEquals("", PushRegistration().tokenHint())
    }
}
