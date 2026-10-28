package dev.proflyder.home.core.config

import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.flow.StateFlow

/**
 * User-configured connection settings. The token is held only in memory after
 * being decrypted from Android Keystore-backed storage; [toString] is
 * deliberately redacted so accidental diagnostics cannot leak it.
 */
data class ConnectionConfig(
    val endpoint: ServerEndpoint? = null,
    val bearerToken: String? = null,
    val connectTimeoutMillis: Long = DEFAULT_CONNECT_TIMEOUT_MILLIS,
    val readTimeoutMillis: Long = DEFAULT_READ_TIMEOUT_MILLIS,
) {
    val isConfigured: Boolean get() = endpoint != null
    val hasToken: Boolean get() = isTokenValid(bearerToken)

    /**
     * Storage validity: endpoint and timeouts plus, when present, a header-safe
     * token. The user may deliberately clear the stored token; such a config is
     * persistable but not request-ready.
     */
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

    /**
     * Request readiness. The scheduler is never authless: an absent or unsafe
     * token is a controlled validation failure before any HTTP request.
     */
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
            "token=${if (hasToken) "<set>" else "<unset>"}, " +
            "connectTimeoutMillis=$connectTimeoutMillis, readTimeoutMillis=$readTimeoutMillis)"

    companion object {
        const val MAX_TOKEN_LENGTH = 4_096

        /**
         * Header-safe bearer token: visible ASCII only, no whitespace or
         * control characters (which would allow header injection or make
         * OkHttp reject the request).
         */
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

/** Whether the encrypted token could be read on this device. */
enum class TokenStatus {
    ABSENT,
    AVAILABLE,
    UNREADABLE,
}

interface ConnectionSettingsStore {
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>

    /** Persists settings; the token is never written in plaintext. */
    suspend fun update(config: ConnectionConfig): Outcome<Unit>
}
