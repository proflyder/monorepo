package dev.proflyder.home.core.settings

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidConnectionSettingsStore(
    dataStore: DataStore<Preferences>,
    cipher: TokenCipher,
    remote: RemoteConnectionSource,
    scope: CoroutineScope,
    @IoDispatcher io: CoroutineDispatcher,
) : ConnectionSettingsStore {
    private val stored = StoredTokenSource(dataStore, cipher, io, scope)

    override val tokenStatus: StateFlow<TokenStatus> = stored.state
        .map { it.status }
        .stateIn(scope, SharingStarted.Eagerly, TokenStatus.LOADING)

    override val config: StateFlow<ConnectionConfig> = combine(remote.state, stored.state) { state, token ->
        compose(state, token.token)
    }.stateIn(scope, SharingStarted.Eagerly, compose(remote.state.value, null))

    override val loaded: Flow<Boolean> = combine(remote.state, stored.state) { state, token ->
        state != RemoteConnectionState.Loading && token.status != TokenStatus.LOADING
    }.distinctUntilChanged()

    override suspend fun updateToken(bearerToken: String?): Outcome<Unit> {
        val token = bearerToken?.trim()?.takeIf { it.isNotEmpty() }
        if (token != null && !ConnectionConfig.isTokenValid(token)) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
        }
        val written = stored.write(token)
        if (written is Outcome.Success) config.first { it.bearerToken == token }
        return written
    }

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
            intercomEndpoint = loaded?.intercomEndpoint,
            secretsEndpoint = loaded?.secretsEndpoint,
            bearerToken = token,
            connectTimeoutMillis = loaded?.connectTimeoutMillis ?: ConnectionConfig.DEFAULT_CONNECT_TIMEOUT_MILLIS,
            readTimeoutMillis = loaded?.readTimeoutMillis ?: ConnectionConfig.DEFAULT_READ_TIMEOUT_MILLIS,
        )
    }
}
