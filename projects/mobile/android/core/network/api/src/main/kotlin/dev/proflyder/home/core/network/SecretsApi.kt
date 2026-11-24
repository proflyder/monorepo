package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SecretCatalog
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpSession

interface SecretsApi {
    suspend fun catalog(): Outcome<SecretCatalog>

    suspend fun metadata(path: String): Outcome<SecretMetadata>

    suspend fun stepUp(totp: String): Outcome<StepUpSession>

    suspend fun reveal(path: String, key: String, version: Long, session: String): Outcome<SecretValue>

    suspend fun update(path: String, key: String, value: String, cas: Long, totp: String): Outcome<SecretVersionRef>

    suspend fun rollback(path: String, version: Long, cas: Long, totp: String): Outcome<SecretRollback>

    suspend fun delete(path: String, cas: Long, version: Long, totp: String): Outcome<Unit>
}
