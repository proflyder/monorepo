package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
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

    suspend fun stepUp(totp: String): Outcome<Unit>

    suspend fun reveal(path: String, key: String, version: Long): Outcome<SecretValue>

    suspend fun update(path: String, key: String, value: String, cas: Long, totp: String): Outcome<SecretVersionRef>

    suspend fun rollback(path: String, version: Long, cas: Long, totp: String): Outcome<SecretRollback>

    suspend fun delete(path: String, cas: Long, version: Long, totp: String): Outcome<Unit>
}

fun stepUpRequiredFailure(): RemoteFailure = RemoteFailure.Server(
    httpStatus = 403,
    code = ConnectCode.UNKNOWN,
    message = STEP_UP_REQUIRED,
    reason = STEP_UP_REQUIRED,
)

const val STEP_UP_REQUIRED: String = "step_up_required"
