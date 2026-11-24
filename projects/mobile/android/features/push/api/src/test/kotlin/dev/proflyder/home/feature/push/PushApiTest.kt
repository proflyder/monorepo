package dev.proflyder.home.feature.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    PushApiTest::class,
)
class AllPushApiTests

class PushApiTest {
    @Test
    fun aHomeDeepLinkIsParsedIntoItsRoute() {
        assertEquals("alarms", PushDeepLink.parse("home://alarms")?.route)
        assertEquals("time/alarms", PushDeepLink.parse("  home://time/alarms/  ")?.route)
        assertEquals(
            "secrets/path/infra%2Fbackup%2Fha",
            PushDeepLink.parse("home://secrets/path/infra%2Fbackup%2Fha")?.route,
        )
        assertEquals("secrets/path/user+tag@host", PushDeepLink.parse("home://secrets/path/user+tag@host")?.route)
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
        val token = listOf("fcm", "registration", "tail123456").joinToString("-")
        val registration = PushRegistration(token = token)
        assertEquals("123456", registration.tokenHint())
        assertEquals("", PushRegistration().tokenHint())
    }
}
