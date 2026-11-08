package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Connect error details carry the server's stable localization key. */
class ConnectErrorLocalizationTest {
    private val json = Json { ignoreUnknownKeys = true }

    private fun fixture(name: String): String =
        checkNotNull(javaClass.getResource("/fixtures/$name")).readText()

    @Test
    fun localizedDetailKeyAndArgsReachTheFailure() {
        val failure = parseConnectFailure(412, fixture("connect_error_localized.json"), json)
        assertTrue(failure is RemoteFailure.Server)
        failure as RemoteFailure.Server
        assertEquals("failed_precondition", failure.code)
        assertEquals("scheduler.error.schedule_not_ringing", failure.textKey)
        assertTrue(failure.textArgs.isEmpty())

        val withArgs = parseConnectFailure(429, fixture("connect_error_args.json"), json)
        withArgs as RemoteFailure.Server
        assertEquals("scheduler.error.timer_limit", withArgs.textKey)
        assertEquals("64", withArgs.textArgs["limit"])
    }

    @Test
    fun legacyErrorWithoutDetailsKeepsTheMessageAndHasNoKey() {
        val failure = parseConnectFailure(500, fixture("connect_error_legacy.json"), json)
        failure as RemoteFailure.Server
        assertEquals("internal error", failure.message)
        assertEquals("", failure.textKey)
        assertTrue(failure.textArgs.isEmpty())
    }
}
