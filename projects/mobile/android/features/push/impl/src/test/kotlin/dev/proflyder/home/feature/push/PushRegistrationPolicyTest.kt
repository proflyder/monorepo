package dev.proflyder.home.feature.push

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PushRegistrationPolicyTest {
    private val registeredAt = 1_000_000L
    private val last = PushRegistration(
        token = "token-a",
        registeredAtMillis = registeredAt,
        appVersion = "1.2.0",
        server = "https://home.test/",
    )
    private val same = PushRegistrationRequest("token-a", "1.2.0", "https://home.test/")

    @Test
    fun anUnchangedFreshRegistrationIsSkipped() {
        assertFalse(PushRegistrationPolicy.isDue(last, same, registeredAt + 60_000))
        val dayEdge = registeredAt + PushRegistrationPolicy.REFRESH_INTERVAL_MILLIS
        assertFalse(PushRegistrationPolicy.isDue(last, same, dayEdge))
    }

    @Test
    fun aNeverRegisteredPhoneRegisters() {
        assertTrue(PushRegistrationPolicy.isDue(PushRegistration(), same, registeredAt))
    }

    @Test
    fun aChangedTokenVersionOrServerRegisters() {
        val now = registeredAt + 60_000
        assertTrue(PushRegistrationPolicy.isDue(last, same.copy(token = "token-b"), now))
        assertTrue(PushRegistrationPolicy.isDue(last, same.copy(appVersion = "1.3.0"), now))
        assertTrue(PushRegistrationPolicy.isDue(last, same.copy(server = "https://other.test/"), now))
    }

    @Test
    fun aRegistrationOlderThanADayIsRefreshed() {
        val stale = registeredAt + PushRegistrationPolicy.REFRESH_INTERVAL_MILLIS + 1
        assertTrue(PushRegistrationPolicy.isDue(last, same, stale))
    }

    @Test
    fun aClockGoingBackwardsRegisters() {
        assertTrue(PushRegistrationPolicy.isDue(last, same, registeredAt - 1))
    }
}
