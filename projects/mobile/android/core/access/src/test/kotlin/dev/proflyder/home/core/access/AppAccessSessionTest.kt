package dev.proflyder.home.core.access

import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.core.testing.FakeSyncSignals
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AppAccessSessionTest {
    private class FakeApi : AccessApi {
        val results = ArrayDeque<Outcome<AppAccess>>()
        var calls = 0

        override suspend fun me(): Outcome<AppAccess> {
            calls++
            return results.removeFirstOrNull() ?: Outcome.Success(access)
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
        }
    }

    @Test
    fun startLoadsTheCurrentTokenAndStaleAccessRefreshesIt() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val signals = FakeSyncSignals()
        val session = AppAccessSession(api, signals, backgroundScope)
        val forbidden = AppAccessState.Ready(AppAccess("Мама", setOf(AppModule.BASE), false))
        api.results.addAll(listOf(Outcome.Success(FakeApi.access), Outcome.Success(forbidden.access)))

        session.start()
        assertEquals(FakeApi.access, (session.state.value as AppAccessState.Ready).access)
        assertTrue(session.state.value.administrator)

        signals.publish(SyncSignal.Stale)
        assertEquals(2, api.calls)
        assertEquals(forbidden.access, (session.state.value as AppAccessState.Ready).access)
        assertFalse(session.state.value.administrator)
    }

    @Test
    fun anOfflineMeKeepsTheLastKnownModulesAndUnknownAllowsEverything() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val session = AppAccessSession(api, FakeSyncSignals(), backgroundScope)
        api.results.addAll(
            listOf(
                Outcome.Success(FakeApi.access),
                Outcome.Failure(RemoteFailure.Offline("down")),
            ),
        )

        session.start()
        session.refresh()
        assertEquals(FakeApi.access, (session.state.value as AppAccessState.Ready).access)

        val fresh = AppAccessSession(FakeApi(), FakeSyncSignals(), backgroundScope)
        assertEquals(AppAccessState.Unknown, fresh.state.value)
        assertTrue(fresh.state.value.allows(AppModule.ADMIN))
    }

    @Test
    fun aModuleForbiddenRefreshesMeButDoesNotHideAnythingByItself() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val session = AppAccessSession(api, FakeSyncSignals(), backgroundScope)
        val restricted = AppAccess("Phone", setOf(AppModule.BASE, AppModule.HOME), false)
        api.results.addAll(listOf(Outcome.Success(FakeApi.access), Outcome.Success(restricted)))

        session.start()
        session.moduleForbidden("admin")

        assertEquals(2, api.calls)
        assertEquals(restricted, (session.state.value as AppAccessState.Ready).access)
        assertFalse(session.state.value.allows(AppModule.SYSTEM))
        assertTrue(session.state.value.allows(AppModule.HOME))
    }
}
