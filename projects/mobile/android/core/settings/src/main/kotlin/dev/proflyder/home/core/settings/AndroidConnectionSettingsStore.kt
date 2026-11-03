package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

/**
 * Stores the bearer token as Keystore-encrypted ciphertext and composes it with
 * the endpoints/timeouts supplied by [RemoteConnectionSource]. The manifest
 * disables Android backup, so the token never leaves the device.
 *
 * Remote Config is authoritative for endpoints and timeouts: the legacy DataStore
 * fields are no longer read or written (the migration and keys remain so an
 * upgrade keeps the encrypted token). Requests read [config] synchronously
 * (interceptors run on I/O threads), so [config] keeps a synchronous snapshot.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidConnectionSettingsStore(
    private val dataStore: DataStore<Preferences>,
    private val cipher: TokenCipher,
    private val remote: RemoteConnectionSource,
) : ConnectionSettingsStore {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val initial: Preferences = runBlocking {
        dataStore.data.catch { if (it is IOException) emit(emptyPreferences()) else throw it }.first()
    }

    private val initialToken: String? = initial[KEY_TOKEN_CIPHERTEXT]
        ?.let(cipher::decrypt)
        ?.takeIf(ConnectionConfig::isTokenValid)

    private val mutableTokenStatus = MutableStateFlow(
        when {
            !initial.contains(KEY_TOKEN_CIPHERTEXT) -> TokenStatus.ABSENT
            initialToken != null -> TokenStatus.AVAILABLE
            else -> TokenStatus.UNREADABLE
        },
    )
    override val tokenStatus: StateFlow<TokenStatus> = mutableTokenStatus.asStateFlow()

    private val mutableConfig = MutableStateFlow(compose(remote.state.value, initialToken))
    override val config: StateFlow<ConnectionConfig> = mutableConfig.asStateFlow()

    init {
        scope.launch {
            remote.state.collect { state ->
                mutableConfig.value = compose(state, mutableConfig.value.bearerToken)
            }
        }
    }

    override suspend fun updateToken(bearerToken: String?): Outcome<Unit> {
        val normalizedToken = bearerToken?.trim()?.takeIf { it.isNotEmpty() }
        if (normalizedToken != null && !ConnectionConfig.isTokenValid(normalizedToken)) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }

        val encrypted = if (normalizedToken != null) {
            try {
                cipher.encrypt(normalizedToken)
            } catch (_: Exception) {
                return Outcome.Failure(RemoteFailure.Storage("The token could not be encrypted on this device."))
            }
        } else {
            null
        }

        try {
            dataStore.edit { prefs ->
                if (encrypted != null) prefs[KEY_TOKEN_CIPHERTEXT] = encrypted else prefs.remove(KEY_TOKEN_CIPHERTEXT)
            }
        } catch (_: IOException) {
            return Outcome.Failure(RemoteFailure.Storage("Settings could not be saved on this device."))
        }

        mutableConfig.value = compose(remote.state.value, normalizedToken)
        mutableTokenStatus.value = if (normalizedToken != null) TokenStatus.AVAILABLE else TokenStatus.ABSENT
        return Outcome.Success(Unit)
    }

    /** Endpoints/timeouts come only from Remote Config; the token is device-local. */
    private fun compose(state: RemoteConnectionState, token: String?): ConnectionConfig {
        val loaded = when (state) {
            is RemoteConnectionState.Loaded -> state
            is RemoteConnectionState.FetchFailed -> state.lastKnown
            RemoteConnectionState.Unavailable,
            RemoteConnectionState.Loading,
            RemoteConnectionState.Missing,
            is RemoteConnectionState.Invalid,
            -> null
        }
        return ConnectionConfig(
            endpoint = loaded?.endpoint,
            controlEndpoint = loaded?.controlEndpoint,
            bearerToken = token,
            connectTimeoutMillis = loaded?.connectTimeoutMillis
                ?: ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS,
            readTimeoutMillis = loaded?.readTimeoutMillis
                ?: ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS,
        )
    }

    internal companion object {
        /**
         * Legacy key names copied from the pre-DataStore SharedPreferences file.
         * They are kept only so the migration remains compatible; endpoints and
         * timeouts are no longer read from or written to DataStore.
         */
        val KEY_ENDPOINT = stringPreferencesKey("endpoint")
        val KEY_CONTROL_ENDPOINT = stringPreferencesKey("control_endpoint")
        val KEY_CONNECT_TIMEOUT = longPreferencesKey("connect_timeout_millis")
        val KEY_READ_TIMEOUT = longPreferencesKey("read_timeout_millis")

        val KEY_TOKEN_CIPHERTEXT = stringPreferencesKey("bearer_token_ciphertext")
    }
}
