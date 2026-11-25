package dev.proflyder.home.feature.access

import dev.proflyder.home.core.access.AccessRefresher
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.feature.access.data.RemoteAccessRepository
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RemoteAccessRepositoryTest {
    private class FakeApi : AccessApi {
        var tokensResult: Outcome<List<AccessToken>> = Outcome.Success(listOf(phone))
        var createResult: Outcome<CreatedAccessToken> = Outcome.Success(CreatedAccessToken("home_new", tablet))
        var updateResult: Outcome<AccessToken> = Outcome.Success(phone.copy(name = "Кухня", modules = setOf(AppModule.BASE)))
        var revokeResult: Outcome<Unit> = Outcome.Success(Unit)

        override suspend fun me(): Outcome<dev.proflyder.home.core.model.AppAccess> =
            Outcome.Failure(RemoteFailure.Unauthorized)

        var pending: CompletableDeferred<Outcome<List<AccessToken>>>? = null

        override suspend fun tokens(): Outcome<List<AccessToken>> = pending?.await() ?: tokensResult

        override suspend fun createToken(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken> =
            createResult

        override suspend fun updateToken(
            id: String,
            name: String?,
            modules: Set<AppModule>?,
        ): Outcome<AccessToken> = updateResult

        override suspend fun revokeToken(id: String): Outcome<Unit> = revokeResult
    }

    private class FakeRefresher : AccessRefresher {
        var refreshes = 0

        override fun refresh() {
            refreshes++
        }
    }

    @Test
    fun theStoreLoadsOnceAppendsReplacesAndRemovesLocally() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val refresher = FakeRefresher()
        val repository = RemoteAccessRepository(api, refresher, backgroundScope)

        assertEquals(listOf(phone), repository.state.value.tokens)
        assertEquals(false, repository.state.value.loading)

        repository.create("Планшет", setOf(AppModule.HOME))
        assertEquals(listOf(phone, tablet), repository.state.value.tokens)

        repository.update(phone.id, "Кухня", setOf(AppModule.BASE))
        assertEquals("Кухня", repository.state.value.tokens.first().name)

        repository.revoke(phone.id)
        assertEquals(listOf(tablet), repository.state.value.tokens)
        assertEquals(4, refresher.refreshes)
    }

    @Test
    fun aFailedRefreshKeepsTheRowsAndExposesTheFailure() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val repository = RemoteAccessRepository(api, FakeRefresher(), backgroundScope)
        api.tokensResult = Outcome.Failure(RemoteFailure.ModuleForbidden("admin"))

        repository.refresh()

        assertEquals(listOf(phone), repository.state.value.tokens)
        assertTrue(repository.state.value.failure is RemoteFailure.ModuleForbidden)
        assertEquals(false, repository.state.value.loading)
    }

    @Test
    fun aRefreshThatReplacesAnInFlightLoadShowsNoFailure() = runTest(UnconfinedTestDispatcher()) {
        val api = FakeApi()
        val repository = RemoteAccessRepository(api, FakeRefresher(), backgroundScope)
        api.pending = CompletableDeferred()
        repository.refresh()
        api.pending = CompletableDeferred()

        repository.refresh()

        assertNull(repository.state.value.failure)
        assertEquals(true, repository.state.value.loading)
    }

    private companion object {
        val phone = AccessToken(
            id = "t-phone",
            name = "Телефон",
            modules = setOf(AppModule.BASE, AppModule.HOME),
            owner = false,
            createdUnix = 1_700_000_000L,
            updatedUnix = 1_700_000_000L,
        )

        val tablet = AccessToken(
            id = "t-new",
            name = "Планшет",
            modules = setOf(AppModule.BASE, AppModule.HOME),
            owner = false,
            createdUnix = 1_700_000_100L,
            updatedUnix = 1_700_000_100L,
        )
    }
}
