package dev.proflyder.home.core.access

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.testing.FakeSyncSignals
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AppAccessSessionTest {
    private class FakeSettings(token: String?) : ConnectionSettingsStore {
        private val mutable = MutableStateFlow(ConnectionConfig(bearerToken = token))
        override val config: StateFlow<ConnectionConfig> = mutable
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.AVAILABLE)

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)

        fun setToken(token: String?) {
            mutable.value = mutable.value.copy(bearerToken = token)
        }
    }

    private class FakeApi : AccessApi {
        val results = ArrayDeque<Outcome<AppAccess>>()
        var calls = 0
        var gate: CompletableDeferred<Unit>? = null
        var next: Outcome<AppAccess> = Outcome.Success(access)

        override suspend fun me(): Outcome<AppAccess> {
            calls++
            gate?.await()
            return results.removeFirstOrNull() ?: next
        }

        override suspend fun tokens(): Outcome<List<AccessToken>> = Outcome.Success(emptyList())

        override suspend fun createToken(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken> =
            Outcome.Failure(RemoteFailure.Unauthorized)

        override suspend fun updateToken(
            id: String,
            name: String?,
            modules: Set<AppModule>?,
        ): Outcome<AccessToken> = Outcome.Failure(RemoteFailure.Unauthorized)

        override suspend fun revokeToken(id: String): Outcome<Unit> = Outcome.Failure(RemoteFailure.Unauthorized)

        companion object {
            val access = AppAccess(
                name = "Owner",
                modules = setOf(AppModule.BASE, AppModule.SYSTEM, AppModule.ADMIN),
                owner = true,
            )

            val restricted = AppAccess(
                name = "Phone",
                modules = setOf(AppModule.BASE, AppModule.HOME),
                owner = false,
            )
        }
    }

    private fun session(
        api: FakeApi,
        settings: FakeSettings,
        signals: FakeSyncSignals,
        clock: FakeClock,
        scope: kotlinx.coroutines.CoroutineScope,
    ): AppAccessSession = AppAccessSession(api, settings, signals, clock, scope)

    @Test
    fun startLoadsTheCurrentTokenAndStaleAccessRefreshesIt() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val signals = FakeSyncSignals()
        val settings = FakeSettings("owner-token")
        val session = session(api, settings, signals, FakeClock(), backgroundScope)
        val restricted = AppAccess("Мама", setOf(AppModule.BASE), false)
        api.results.addAll(
            listOf(Outcome.Success(FakeApi.access), Outcome.Success(restricted), Outcome.Success(restricted)),
        )

        session.start()
        assertEquals(FakeApi.access, (session.state.value as AppAccessState.Ready).access)
        assertTrue(session.state.value.administrator)

        signals.publish(SyncSignal.Stale)
        assertEquals(2, api.calls)
        assertEquals(restricted, (session.state.value as AppAccessState.Ready).access)
        assertFalse(session.state.value.administrator)
    }

    @Test
    fun anOfflineMeKeepsTheLastKnownModulesAndUnknownAllowsEverything() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val settings = FakeSettings("owner-token")
        val session = session(api, settings, FakeSyncSignals(), FakeClock(), backgroundScope)
        api.results.addAll(
            listOf(
                Outcome.Success(FakeApi.access),
                Outcome.Failure(RemoteFailure.Offline("down")),
            ),
        )

        session.start()
        session.refresh()
        assertEquals(FakeApi.access, (session.state.value as AppAccessState.Ready).access)

        val fresh = session(FakeApi(), FakeSettings(null), FakeSyncSignals(), FakeClock(), backgroundScope)
        assertEquals(AppAccessState.Unknown, fresh.state.value)
        assertTrue(fresh.state.value.allows(AppModule.ADMIN))
    }

    @Test
    fun aModuleForbiddenRefreshesMeAtMostOncePerWindow() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val clock = FakeClock()
        val session = session(api, FakeSettings("phone"), FakeSyncSignals(), clock, backgroundScope)
        val restricted = AppAccess("Phone", setOf(AppModule.BASE, AppModule.HOME), false)
        api.results.addAll(
            listOf(Outcome.Success(FakeApi.access), Outcome.Success(restricted), Outcome.Success(restricted)),
        )

        session.start()
        session.moduleForbidden("admin")
        assertEquals(2, api.calls)
        assertEquals(restricted, (session.state.value as AppAccessState.Ready).access)

        session.moduleForbidden("chat")
        session.moduleForbidden("vpn")
        assertEquals(2, api.calls)

        clock.advanceMillis(3_000)
        session.moduleForbidden("chat")
        assertEquals(3, api.calls)
        assertFalse(session.state.value.allows(AppModule.SYSTEM))
    }

    @Test
    fun aTokenChangeResetsReadyBeforeTheNewMeArrivesAndClearedTokenNeverRequests() =
        runTest(StandardTestDispatcher()) {
            val api = FakeApi()
            val settings = FakeSettings("owner-token")
            val session = session(api, settings, FakeSyncSignals(), FakeClock(), backgroundScope)
            session.start()
            runCurrent()
            assertEquals(FakeApi.access, (session.state.value as AppAccessState.Ready).access)

            val gate = CompletableDeferred<Unit>()
            api.gate = gate
            api.next = Outcome.Success(FakeApi.restricted)
            settings.setToken("phone-token")
            runCurrent()
            assertEquals(AppAccessState.Unknown, session.state.value)
            assertEquals(2, api.calls)

            gate.complete(Unit)
            runCurrent()
            assertEquals(FakeApi.restricted, (session.state.value as AppAccessState.Ready).access)

            settings.setToken(null)
            runCurrent()
            assertEquals(AppAccessState.Unknown, session.state.value)
            assertEquals(2, api.calls)
        }

    @Test
    fun aReadyStateDoesNotSurviveACancelledRequestForThePreviousToken() =
        runTest(StandardTestDispatcher()) {
            val api = FakeApi()
            val settings = FakeSettings("owner-token")
            val session = session(api, settings, FakeSyncSignals(), FakeClock(), backgroundScope)
            session.start()
            runCurrent()

            val stale = CompletableDeferred<Unit>()
            api.gate = stale
            api.next = Outcome.Success(FakeApi.access)
            settings.setToken("phone-token")
            runCurrent()

            api.gate = null
            api.next = Outcome.Success(FakeApi.restricted)
            settings.setToken("second-token")
            runCurrent()
            stale.complete(Unit)
            runCurrent()

            assertEquals(FakeApi.restricted, (session.state.value as AppAccessState.Ready).access)
        }
}
