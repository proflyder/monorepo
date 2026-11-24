package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpChallenge
import kotlinx.coroutines.flow.StateFlow

data class SecretsCatalogState(
    val loading: Boolean = true,
    val entries: List<SecretEntry> = emptyList(),
    val truncated: Boolean = false,
    val failure: RemoteFailure? = null,
)

interface SecretsRepository {
    val catalog: StateFlow<SecretsCatalogState>

    val stepUpActive: StateFlow<Boolean>

    fun refresh()

    suspend fun metadata(path: String): Outcome<SecretMetadata>

    suspend fun device(): Outcome<DeviceEnrollment>

    suspend fun deviceChallenge(): Outcome<DeviceAttestationChallenge>

    suspend fun enroll(
        code: String,
        publicKey: String,
        attestationChallenge: String,
        attestationChain: List<String>,
    ): Outcome<DeviceEnrollment>

    suspend fun challenge(
        operation: SecretOperation,
        path: String?,
        key: String?,
        version: Long?,
        cas: Long?,
    ): Outcome<StepUpChallenge>

    suspend fun stepUp(challenge: String, signature: String): Outcome<Unit>

    suspend fun reveal(path: String, key: String, version: Long): Outcome<SecretValue>

    suspend fun update(
        path: String,
        key: String,
        value: String,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretVersionRef>

    suspend fun rollback(
        path: String,
        version: Long,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretRollback>

    suspend fun delete(
        path: String,
        cas: Long,
        version: Long,
        challenge: String,
        signature: String,
    ): Outcome<Unit>
}

fun stepUpRequiredFailure(): RemoteFailure = RemoteFailure.Server(
    httpStatus = 403,
    code = ConnectCode.UNKNOWN,
    message = STEP_UP_REQUIRED,
    reason = STEP_UP_REQUIRED,
)

const val STEP_UP_REQUIRED: String = "step_up_required"
