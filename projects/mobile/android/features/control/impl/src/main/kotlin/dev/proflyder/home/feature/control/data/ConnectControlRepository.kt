package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.core.network.ControlJson
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.feature.control.ControlRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.UUID

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class ConnectControlRepository(
    private val settings: ConnectionSettingsStore,
    private val catalogClient: ControlCatalogClient,
    private val scheduler: SchedulerClient,
) : ControlRepository {
    override val isConfigured: Boolean get() = settings.config.value.controlEndpoint != null

    override suspend fun catalog(): Outcome<ControlCatalog> = catalogClient.fetch()

    override suspend fun send(entryId: String, action: ControlAction): Outcome<Unit> =
        when (
            val queued = scheduler.enqueueTask(
                taskType = ControlJson.TASK_TYPE,
                queue = ControlJson.QUEUE,
                payloadJson = ControlJson.commandPayload(entryId, action),
                // One tap = one command; the key only protects a transport retry of that tap.
                idempotencyKey = UUID.randomUUID().toString(),
            )
        ) {
            is Outcome.Success -> Outcome.Success(Unit)
            is Outcome.Failure -> queued
        }
}
