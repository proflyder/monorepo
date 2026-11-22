package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ConnectErrorLocalizationTest {
    private fun fixture(name: String): String =
        checkNotNull(javaClass.getResource("/fixtures/$name")).readText()

    @Test
    fun localizedDetailKeyAndArgsReachTheFailure() {
        val failure = parseConnectFailure(412, fixture("connect_error_localized.json"))
        assertTrue(failure is RemoteFailure.Server)
        failure as RemoteFailure.Server
        assertEquals(ConnectCode.FAILED_PRECONDITION, failure.code)
        assertEquals("scheduler.error.schedule_not_ringing", failure.textKey)
        assertTrue(failure.textArgs.isEmpty())

        val withArgs = parseConnectFailure(429, fixture("connect_error_args.json"))
        withArgs as RemoteFailure.Server
        assertEquals("scheduler.error.timer_limit", withArgs.textKey)
        assertEquals("64", withArgs.textArgs["limit"])
    }

    @Test
    fun legacyErrorWithoutDetailsKeepsTheMessageAndHasNoKey() {
        val failure = parseConnectFailure(500, fixture("connect_error_legacy.json"))
        failure as RemoteFailure.Server
        assertEquals("internal error", failure.message)
        assertEquals("", failure.textKey)
        assertTrue(failure.textArgs.isEmpty())
    }
}
