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
    /**
     * Optional base URL of the home control service (device/scenario catalog).
     * The scheduler bearer token is also sent to it.
     */
    val controlEndpoint: ServerEndpoint? = null,
    /**
     * Optional base URL of the intercom signaling service (calls). The phone
     * bearer token sent there is the separate [intercomToken], not the
     * scheduler token: the intercom service authorizes the two independently.
     */
    val intercomEndpoint: ServerEndpoint? = null,
    val bearerToken: String? = null,
    val intercomToken: String? = null,
    val connectTimeoutMillis: Long = DEFAULT_CONNECT_TIMEOUT_MILLIS,
    val readTimeoutMillis: Long = DEFAULT_READ_TIMEOUT_MILLIS,
) {
    val isConfigured: Boolean get() = endpoint != null
    val isIntercomConfigured: Boolean
        get() = intercomEndpoint != null && isTokenValid(intercomToken)
    val hasToken: Boolean get() = isTokenValid(bearerToken)
    val hasIntercomToken: Boolean get() = isTokenValid(intercomToken)

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
        if (intercomToken != null && !isTokenValid(intercomToken)) {
            return Outcome.Failure(RemoteFailure.Validation(Field.INTERCOM_TOKEN))
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
            "controlEndpoint=${controlEndpoint?.baseUrl() ?: "<unset>"}, " +
            "intercomEndpoint=${intercomEndpoint?.baseUrl() ?: "<unset>"}, " +
            "token=${if (hasToken) "<set>" else "<unset>"}, " +
            "intercomToken=${if (hasIntercomToken) "<set>" else "<unset>"}, " +
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
    /**
     * Synchronous snapshot composed from the Remote Config endpoints/timeouts
     * and the device-local token. Interceptors read [StateFlow.value] on I/O
     * threads.
     */
    val config: StateFlow<ConnectionConfig>
    val tokenStatus: StateFlow<TokenStatus>
    /** Whether the separately stored intercom phone token could be read. */
    val intercomTokenStatus: StateFlow<TokenStatus>

    /**
     * Persists only the bearer token (`null` clears it); the token is never
     * written in plaintext. Endpoints and timeouts are owned by Remote Config.
     */
    suspend fun updateToken(bearerToken: String?): Outcome<Unit>

    /** Persists the separate intercom phone token (`null` clears it). */
    suspend fun updateIntercomToken(intercomToken: String?): Outcome<Unit>
}
