package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SecretCatalog
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpSession
import dev.proflyder.home.core.network.SecretsApi
import dev.proflyder.home.feature.secrets.data.RemoteSecretsRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RemoteSecretsRepositoryTest {
    private class FakeApi : SecretsApi {
        var reveals = 0

        override suspend fun catalog(): Outcome<SecretCatalog> = Outcome.Success(SecretCatalog(emptyList(), false))

        override suspend fun metadata(path: String): Outcome<SecretMetadata> =
            Outcome.Success(FakeSecretsRepository.metadata())

        override suspend fun stepUp(totp: String): Outcome<StepUpSession> =
            Outcome.Success(StepUpSession("session-1", 300))

        override suspend fun reveal(
            path: String,
            key: String,
            version: Long,
            session: String,
        ): Outcome<SecretValue> {
            reveals++
            return Outcome.Success(SecretValue(path, version, key, "value"))
        }

        override suspend fun update(
            path: String,
            key: String,
            value: String,
            cas: Long,
            totp: String,
        ): Outcome<SecretVersionRef> = Outcome.Success(SecretVersionRef(path, cas + 1, key))

        override suspend fun rollback(
            path: String,
            version: Long,
            cas: Long,
            totp: String,
        ): Outcome<SecretRollback> = Outcome.Success(SecretRollback(path, version, cas + 1))

        override suspend fun delete(path: String, cas: Long, version: Long, totp: String): Outcome<Unit> =
            Outcome.Success(Unit)
    }

    @Test
    fun theRevealSessionExpiresAfterFiveMinutes() = runTest {
        val api = FakeApi()
        val repository = RemoteSecretsRepository(api, FakeClock(), TestScope(testScheduler))

        repository.stepUp("123456")
        assertTrue(repository.stepUpActive.value)

        advanceTimeBy(300_001)
        runCurrent()

        assertFalse(repository.stepUpActive.value)
        val failure = repository.reveal(FakeSecretsRepository.PATH, "token", 3).failureOrNull()
        assertEquals("step_up_required", (failure as dev.proflyder.home.core.model.RemoteFailure.Server).reason)
        assertEquals(0, api.reveals)
    }

    @Test
    fun aSessionAllowsRepeatedRevealsWithinItsWindow() = runTest {
        val api = FakeApi()
        val repository = RemoteSecretsRepository(api, FakeClock(), TestScope(testScheduler))
        repository.stepUp("123456")

        assertTrue(repository.reveal(FakeSecretsRepository.PATH, "token", 3) is Outcome.Success)
        assertTrue(repository.reveal(FakeSecretsRepository.PATH, "endpoint", 3) is Outcome.Success)
        assertEquals(2, api.reveals)
    }
}
