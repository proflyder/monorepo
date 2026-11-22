package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.ControlCatalogClient
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
        enqueue(ControlTaskPayloads.commandPayload(entryId, action))

    override suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit> =
        enqueue(ControlTaskPayloads.adjustPayload(entryId, adjust))

    override suspend fun speak(entryId: String, text: String): Outcome<Unit> =
        enqueue(ControlTaskPayloads.speakPayload(entryId, text))

    override suspend fun setVolume(entryId: String, level: Int): Outcome<Unit> =
        enqueue(ControlTaskPayloads.volumePayload(entryId, level))

    override suspend fun setVolumeMuted(entryId: String, muted: Boolean): Outcome<Unit> =
        enqueue(ControlTaskPayloads.mutePayload(entryId, muted))

    private suspend fun enqueue(payloadJson: String): Outcome<Unit> =
        when (
            val queued = scheduler.enqueueTask(
                taskType = ControlTaskPayloads.TASK_TYPE,
                queue = ControlTaskPayloads.QUEUE,
                payloadJson = payloadJson,
                idempotencyKey = UUID.randomUUID().toString(),
            )
        ) {
            is Outcome.Success -> Outcome.Success(Unit)
            is Outcome.Failure -> queued
        }
}
