package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.StateFlow

data class ConnectionConfig(
    val endpoint: ServerEndpoint? = null,
    val controlEndpoint: ServerEndpoint? = null,
    val intercomEndpoint: ServerEndpoint? = null,
    val bearerToken: String? = null,
    val connectTimeoutMillis: Long = DEFAULT_CONNECT_TIMEOUT_MILLIS,
    val readTimeoutMillis: Long = DEFAULT_READ_TIMEOUT_MILLIS,
) {
    val isConfigured: Boolean get() = endpoint != null
    val isIntercomConfigured: Boolean
        get() = intercomEndpoint != null && isTokenValid(bearerToken)
    val hasToken: Boolean get() = isTokenValid(bearerToken)

    fun validatedForStorage(): Outcome<ConnectionConfig> {
        if (endpoint == null) {
            return Outcome.Failure(RemoteFailure.Validation(Field.ENDPOINT))
        }
        if (connectTimeoutMillis !in MIN_TIMEOUT_MILLIS..MAX_TIMEOUT_MILLIS) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TIMEOUT))
        }
        if (readTimeoutMillis !in MIN_TIMEOUT_MILLIS..MAX_TIMEOUT_MILLIS) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TIMEOUT))
        }
        if (bearerToken != null && !isTokenValid(bearerToken)) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }
        return Outcome.Success(this)
    }

    fun validated(): Outcome<ConnectionConfig> {
        val storage = validatedForStorage()
        if (storage is Outcome.Failure) return storage
        if (!isTokenValid(bearerToken)) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }
        return Outcome.Success(this)
    }

    override fun toString(): String =
        "ConnectionConfig(endpoint=${endpoint?.baseUrl() ?: "<unset>"}, " +
            "controlEndpoint=${controlEndpoint?.baseUrl() ?: "<unset>"}, " +
            "intercomEndpoint=${intercomEndpoint?.baseUrl() ?: "<unset>"}, " +
            "token=${if (hasToken) "<set>" else "<unset>"}, " +
            "connectTimeoutMillis=$connectTimeoutMillis, readTimeoutMillis=$readTimeoutMillis)"

    companion object {
        const val MAX_TOKEN_LENGTH = 4_096

        fun isTokenValid(token: String?): Boolean {
            val value = token?.trim().orEmpty()
            if (value.isEmpty() || value.length > MAX_TOKEN_LENGTH) return false
            return value.all { it.code in 0x21..0x7E }
        }

        const val DEFAULT_CONNECT_TIMEOUT_MILLIS = 5_000L
        const val DEFAULT_READ_TIMEOUT_MILLIS = 20_000L
        const val MIN_TIMEOUT_MILLIS = 1_000L
        const val MAX_TIMEOUT_MILLIS = 120_000L
    }
}

enum class TokenStatus {
    ABSENT,
    AVAILABLE,
    UNREADABLE,
}

interface ConnectionSettingsStore {
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>

    suspend fun updateToken(bearerToken: String?): Outcome<Unit>
}
