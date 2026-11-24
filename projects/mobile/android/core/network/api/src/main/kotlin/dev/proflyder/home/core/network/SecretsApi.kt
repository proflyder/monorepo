package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SecretCatalog
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpChallenge
import dev.proflyder.home.core.model.StepUpSession

interface SecretsApi {
    suspend fun catalog(): Outcome<SecretCatalog>

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
        valueHash: String?,
    ): Outcome<StepUpChallenge>

    suspend fun stepUp(challenge: String, signature: String): Outcome<StepUpSession>

    suspend fun reveal(path: String, key: String, version: Long, session: String): Outcome<SecretValue>

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
