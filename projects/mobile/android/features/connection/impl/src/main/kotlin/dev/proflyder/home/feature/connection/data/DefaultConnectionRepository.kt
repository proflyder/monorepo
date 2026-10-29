package dev.proflyder.home.feature.connection.data

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.connection.ConnectionDraft
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
    private val client: SchedulerClient,
) : ConnectionRepository {
    override val config: StateFlow<ConnectionConfig> = settings.config
    override val tokenStatus: StateFlow<TokenStatus> = settings.tokenStatus

    override suspend fun save(draft: ConnectionDraft): Outcome<ConnectionConfig> {
        val endpoint = when (val parsed = ServerEndpoint.parse(draft.endpointText)) {
            is EndpointParseResult.Valid -> parsed.endpoint
            is EndpointParseResult.Invalid -> {
                return Outcome.Failure(RemoteFailure.Validation(Field.ENDPOINT))
            }
        }
        val controlEndpoint = if (draft.controlEndpointText.isBlank()) {
            null
        } else {
            when (val parsed = ServerEndpoint.parse(draft.controlEndpointText)) {
                is EndpointParseResult.Valid -> parsed.endpoint
                is EndpointParseResult.Invalid -> {
                    return Outcome.Failure(RemoteFailure.Validation(Field.CONTROL_ENDPOINT))
                }
            }
        }
        if (draft.connectTimeoutSeconds !in 1..120 || draft.readTimeoutSeconds !in 1..120) {
            return Outcome.Failure(RemoteFailure.Validation(Field.TIMEOUT))
        }

        val existing = settings.config.value
        val token = when (val update = draft.tokenUpdate) {
            TokenUpdate.Keep -> when (settings.tokenStatus.value) {
                TokenStatus.UNREADABLE -> return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
                else -> existing.bearerToken
            }
            TokenUpdate.Clear -> null
            is TokenUpdate.Replace -> {
                val token = update.value.trim()
                // Reject control characters and whitespace: they cannot be a
                // bearer token and would make OkHttp reject the header.
                if (token.isEmpty() || token.any { it.code !in 0x21..0x7E }) {
                    return Outcome.Failure(RemoteFailure.Validation(Field.TOKEN))
                }
                token
            }
        }

        val config = ConnectionConfig(
            endpoint = endpoint,
            controlEndpoint = controlEndpoint,
            bearerToken = token,
            connectTimeoutMillis = draft.connectTimeoutSeconds * 1000L,
            readTimeoutMillis = draft.readTimeoutSeconds * 1000L,
        )
        return when (val stored = settings.update(config)) {
            is Outcome.Success -> Outcome.Success(settings.config.value)
            is Outcome.Failure -> stored
        }
    }

    override suspend fun testConnection(): Outcome<SchedulerStatus> = client.getStatus()
}
