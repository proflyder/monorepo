package dev.proflyder.home.feature.secrets.data

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpChallenge
import dev.proflyder.home.core.model.StepUpOperation
import dev.proflyder.home.core.network.SecretsApi
import dev.proflyder.home.feature.secrets.SecretsCatalogState
import dev.proflyder.home.feature.secrets.SecretsRepository
import dev.proflyder.home.feature.secrets.stepUpRequiredFailure
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class RemoteSecretsRepository(
    private val api: SecretsApi,
    private val clock: Clock,
    private val scope: CoroutineScope,
) : SecretsRepository {
    private val catalogState = MutableStateFlow(SecretsCatalogState())
    private val stepUpState = MutableStateFlow(false)
    private var session: String? = null
    private var expiresAtNanos: Long = 0
    private var expiry: Job? = null
    private var loading: Job? = null

    override val catalog: StateFlow<SecretsCatalogState> = catalogState.asStateFlow()

    override val stepUpActive: StateFlow<Boolean> = stepUpState.asStateFlow()

    override fun refresh() {
        catalogState.update { it.copy(loading = true, failure = null) }
        loading?.cancel()
        loading = scope.launch { loadCatalog() }
    }

    override suspend fun metadata(path: String): Outcome<SecretMetadata> = api.metadata(path)

    override suspend fun device(): Outcome<DeviceEnrollment> = api.device()

    override suspend fun deviceChallenge(): Outcome<DeviceAttestationChallenge> = api.deviceChallenge()

    override suspend fun enroll(
        code: String,
        publicKey: String,
        attestationChallenge: String,
        attestationChain: List<String>,
    ): Outcome<DeviceEnrollment> =
        api.enroll(code, publicKey, attestationChallenge, attestationChain)

    override suspend fun challenge(operation: StepUpOperation): Outcome<StepUpChallenge> = api.challenge(operation)

    override suspend fun stepUp(challenge: String, signature: String): Outcome<Unit> =
        when (val result = api.stepUp(challenge, signature)) {
            is Outcome.Success -> {
                startSession(result.value.session, result.value.expiresInSeconds)
                Outcome.Success(Unit)
            }
            is Outcome.Failure -> result
        }

    override suspend fun reveal(path: String, key: String, version: Long): Outcome<SecretValue> {
        val active = session?.takeIf { clock.elapsedNanos() < expiresAtNanos }
        if (active == null) {
            clearStepUp()
            return Outcome.Failure(stepUpRequiredFailure())
        }
        return api.reveal(path, key, version, active).also { result ->
            if (result.failureOrNull()?.requiresStepUp == true) clearStepUp()
        }
    }

    override suspend fun update(
        path: String,
        key: String,
        value: String,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretVersionRef> = api.update(path, key, value, cas, challenge, signature)

    override suspend fun rollback(
        path: String,
        version: Long,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretRollback> = api.rollback(path, version, cas, challenge, signature)

    override suspend fun delete(
        path: String,
        cas: Long,
        version: Long,
        challenge: String,
        signature: String,
    ): Outcome<Unit> = api.delete(path, cas, version, challenge, signature)

    private fun startSession(session: String, expiresInSeconds: Long) {
        this.session = session
        expiresAtNanos = clock.elapsedNanos() + expiresInSeconds * NANOS_PER_SECOND
        stepUpState.value = true
        expiry?.cancel()
        expiry = scope.launch {
            delay(expiresInSeconds * MILLIS_PER_SECOND)
            if (this@RemoteSecretsRepository.session == session) clearStepUp()
        }
    }

    private suspend fun loadCatalog() {
        val result = api.catalog()
        catalogState.update { state ->
            when (result) {
                is Outcome.Success -> state.copy(
                    loading = false,
                    entries = result.value.entries,
                    truncated = result.value.truncated,
                    failure = null,
                )
                is Outcome.Failure -> state.copy(loading = false, failure = result.failure)
            }
        }
    }

    private fun clearStepUp() {
        session = null
        expiresAtNanos = 0
        stepUpState.value = false
        expiry?.cancel()
        expiry = null
    }

    private companion object {
        const val MILLIS_PER_SECOND = 1_000L
        const val NANOS_PER_SECOND = 1_000_000_000L
    }
}

internal val RemoteFailure.requiresStepUp: Boolean
    get() = this is RemoteFailure.Server && reason == "step_up_required"
