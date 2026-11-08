package dev.proflyder.home.feature.connection.data

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.RemoteConnectionSource
import dev.proflyder.home.core.config.RemoteConnectionState
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.connection.ConnectionRepository
import dev.proflyder.home.feature.connection.TokenUpdate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.StateFlow

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class DefaultConnectionRepository(
    private val settings: ConnectionSettingsStore,
    private val remote: RemoteConnectionSource,
    private val client: SchedulerClient,
) : ConnectionRepository {
    override val config: StateFlow<ConnectionConfig> = settings.config
    override val tokenStatus: StateFlow<TokenStatus> = settings.tokenStatus
    override val intercomTokenStatus: StateFlow<TokenStatus> = settings.intercomTokenStatus
    override val remoteConfig: StateFlow<RemoteConnectionState> = remote.state

    override suspend fun saveToken(update: TokenUpdate): Outcome<ConnectionConfig> {
        val token = when (update) {
            TokenUpdate.Keep -> when (settings.tokenStatus.value) {
                TokenStatus.UNREADABLE -> return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
                else -> settings.config.value.bearerToken
            }
            TokenUpdate.Clear -> null
            is TokenUpdate.Replace -> {
                val value = update.value.trim()
                if (!ConnectionConfig.isTokenValid(value)) {
                    return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
                }
                value
            }
        }

        return when (val stored = settings.updateToken(token)) {
            is Outcome.Success -> Outcome.Success(settings.config.value)
            is Outcome.Failure -> stored
        }
    }

    override suspend fun refreshRemoteConfig() = remote.refresh()

    override suspend fun saveIntercomToken(update: TokenUpdate): Outcome<ConnectionConfig> {
        val token = when (update) {
            TokenUpdate.Keep -> when (settings.intercomTokenStatus.value) {
                TokenStatus.UNREADABLE -> return Outcome.Failure(RemoteFailure.Validation(Field.INTERCOM_TOKEN))
                else -> settings.config.value.intercomToken
            }
            TokenUpdate.Clear -> null
            is TokenUpdate.Replace -> {
                val value = update.value.trim()
                if (!ConnectionConfig.isTokenValid(value)) {
                    return Outcome.Failure(RemoteFailure.Validation(Field.INTERCOM_TOKEN))
                }
                value
            }
        }

        return when (val stored = settings.updateIntercomToken(token)) {
            is Outcome.Success -> Outcome.Success(settings.config.value)
            is Outcome.Failure -> stored
        }
    }

    override suspend fun testConnection(): Outcome<SchedulerStatus> = client.getStatus()
}
