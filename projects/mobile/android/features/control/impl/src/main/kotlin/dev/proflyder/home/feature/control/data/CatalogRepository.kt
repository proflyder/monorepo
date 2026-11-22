package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import dev.proflyder.home.core.model.map
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.core.network.ControlJson
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.feature.control.ControlCatalogState
import dev.proflyder.home.feature.control.ControlRepository
import java.util.UUID
import kotlin.time.Duration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.launch

internal class CatalogRepository(
    settings: ConnectionSettingsStore,
    foreground: AppForeground,
    private val catalogClient: ControlCatalogClient,
    private val scheduler: SchedulerClient,
    private val optimistic: OptimisticDeviceState,
    private val policy: ControlRefreshPolicy,
    private val scope: CoroutineScope,
) : ControlRepository {
    private val triggers = RefreshTriggers(foreground, policy.period)

    companion object {
        fun create(
            settings: ConnectionSettingsStore,
            foreground: AppForeground,
            catalogClient: ControlCatalogClient,
            scheduler: SchedulerClient,
            clock: Clock,
            scope: CoroutineScope,
            policy: ControlRefreshPolicy = ControlRefreshPolicy(),
        ): CatalogRepository = CatalogRepository(
            settings = settings,
            foreground = foreground,
            catalogClient = catalogClient,
            scheduler = scheduler,
            optimistic = OptimisticDeviceState(clock, policy.expectationTtl.inWholeNanoseconds),
            policy = policy,
            scope = scope,
        )
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override val catalog: StateFlow<ControlCatalogState> = settings.config
        .map { it.controlEndpoint != null }
        .distinctUntilChanged()
        .flatMapLatest(::catalogWhen)
        .combine(optimistic.expectations) { state, expected -> state.withOverlay(expected) }
        .stateIn(scope, SharingStarted.WhileSubscribed(policy.stopTimeout.inWholeMilliseconds), ControlCatalogState())

    override fun refresh() = triggers.request()

    override suspend fun send(entryId: String, action: ControlAction): Outcome<Unit> {
        val payload = ControlJson.commandPayload(entryId, action)
        return when (action) {
            ControlAction.TURN_ON -> command(entryId, Expectation(power = PowerState.ON), payload)
            ControlAction.TURN_OFF -> command(entryId, Expectation(power = PowerState.OFF), payload)
            ControlAction.PRESS -> enqueue(payload).also { settle(entryId, policy.pressSettle, untilSettled = false) }
            else -> enqueue(payload)
        }
    }

    override suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit> =
        command(entryId, Expectation(power = PowerState.ON, adjust = adjust), ControlJson.adjustPayload(entryId, adjust))

    override suspend fun speak(entryId: String, text: String): Outcome<Unit> =
        enqueue(ControlJson.speakPayload(entryId, text))

    override suspend fun setVolume(entryId: String, level: Int): Outcome<Unit> =
        volumeCommand(entryId, ControlJson.volumePayload(entryId, level)) { copy(level = level) }

    override suspend fun setVolumeMuted(entryId: String, muted: Boolean): Outcome<Unit> =
        volumeCommand(entryId, ControlJson.mutePayload(entryId, muted)) { copy(muted = muted) }

    private suspend fun volumeCommand(
        entryId: String,
        payload: String,
        change: VolumeControls.() -> VolumeControls,
    ): Outcome<Unit> {
        val current = catalog.value.entries.firstOrNull { it.id == entryId }?.volume
        val expectation = current?.let { Expectation(volume = it.change()) }
        return command(entryId, expectation, payload, policy.volumeSettle)
    }

    private suspend fun command(
        entryId: String,
        expectation: Expectation?,
        payload: String,
        settleSteps: List<Duration> = policy.powerSettle,
    ): Outcome<Unit> {
        expectation?.let { optimistic.expect(entryId, it) }
        val result = enqueue(payload)
        when (result) {
            is Outcome.Success -> settle(entryId, settleSteps, untilSettled = expectation != null)
            is Outcome.Failure -> rollback(entryId)
        }
        return result
    }

    private fun rollback(entryId: String) {
        optimistic.drop(entryId)
        refresh()
    }

    private fun settle(entryId: String, steps: List<Duration>, untilSettled: Boolean) {
        scope.launch { followUp(entryId, steps, untilSettled) }
    }

    private suspend fun followUp(entryId: String, steps: List<Duration>, untilSettled: Boolean) {
        for (step in steps) {
            delay(step)
            if (untilSettled && !optimistic.isPending(entryId)) return
            refresh()
        }
    }

    private fun catalogWhen(configured: Boolean): Flow<ControlCatalogState> = when (configured) {
        true -> triggers.ticks
            .transform { emit(null); emit(fetch()) }
            .runningFold(ControlCatalogState(), ControlCatalogState::after)
        false -> flowOf(ControlCatalogState(configured = false, loading = false))
    }

    private suspend fun fetch(): Outcome<ControlCatalog> = catalogClient.fetch()
        .map { catalog -> catalog.copy(entries = catalog.entries.filter { it.kind != ControlKind.OTHER }) }
        .also { result -> result.getOrNull()?.let { optimistic.settle(it.entries) } }

    private fun ControlCatalogState.withOverlay(expected: Map<String, TimedExpectation>): ControlCatalogState =
        copy(catalog = catalog?.let { it.copy(entries = optimistic.overlay(it.entries, expected)) })

    private suspend fun enqueue(payloadJson: String): Outcome<Unit> = scheduler.enqueueTask(
        taskType = ControlJson.TASK_TYPE,
        queue = ControlJson.QUEUE,
        payloadJson = payloadJson,
        idempotencyKey = UUID.randomUUID().toString(),
    ).map { }
}

internal fun ControlCatalogState.after(result: Outcome<ControlCatalog>?): ControlCatalogState = when (result) {
    null -> copy(configured = true, loading = true)
    is Outcome.Success -> copy(configured = true, loading = false, catalog = result.value, failure = null)
    is Outcome.Failure -> copy(configured = true, loading = false, failure = result.failure)
}
