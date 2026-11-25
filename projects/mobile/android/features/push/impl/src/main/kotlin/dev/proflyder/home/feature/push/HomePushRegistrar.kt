package dev.proflyder.home.feature.push

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.AppVersion
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@ContributesBinding(AppScope::class, binding = binding<PushRegistrar>())
@Inject
class HomePushRegistrar(
    private val store: PushRegistrationStore,
    private val tokens: PushTokenSource,
    private val publisher: PushRegistrationPublisher,
    private val settings: ConnectionSettingsStore,
    private val version: AppVersion,
    private val clock: Clock,
    private val activation: AppActivation,
    private val scope: CoroutineScope,
) : PushRegistrar, AppStartable {
    private val inFlight = InFlight()
    private val failed = MutableStateFlow(false)
    private val registering = Mutex()

    override val status: StateFlow<PushSyncStatus> = combine(inFlight.busy, failed, ::PushSyncStatus)
        .stateIn(scope, SharingStarted.Eagerly, PushSyncStatus())

    override fun start() {
        scope.launch {
            store.load()
            activation.awaitActive()
            wanted().collectLatest { register(it, force = false) }
        }
    }

    override suspend fun registerNow(): PushRegistrationResult {
        val server = PushRegistrationPolicy.serverOf(settings.config.value)
            ?: return PushRegistrationResult.NotConfigured
        val token = tokens.token.value ?: tokens.fetch()
        val request = request(token, server)
            ?: return markFailed(PushRegistrationResult.NoPushToken)
        return register(request, force = true)
    }

    internal fun unregister(token: String) {
        scope.launch { inFlight.run { suspendRunCatching { publisher.unregister(token) } } }
    }

    private fun wanted(): Flow<PushRegistrationRequest> = combine(
        tokens.token.filterNotNull(),
        settings.config.map(PushRegistrationPolicy::serverOf),
        ::request,
    )
        .filterNotNull()
        .distinctUntilChanged()

    private fun request(token: String?, server: String?): PushRegistrationRequest? {
        if (token.isNullOrBlank() || server == null) return null
        return PushRegistrationRequest(token, PushRegistrationPolicy.cleanVersion(version.name), server)
    }

    private suspend fun register(request: PushRegistrationRequest, force: Boolean): PushRegistrationResult =
        registering.withLock { registerLocked(request, force) }

    private suspend fun registerLocked(request: PushRegistrationRequest, force: Boolean): PushRegistrationResult {
        val now = clock.wallMillis()
        if (!force && !PushRegistrationPolicy.isDue(store.registration.value, request, now)) {
            return PushRegistrationResult.Done
        }
        val published = inFlight.run { suspendRunCatching { publisher.publish(request) }.getOrElse(::unexpected) }
        val result = PushRegistrationPolicy.resultOf(published)
        failed.value = result != PushRegistrationResult.Done
        if (result == PushRegistrationResult.Done) store.saved(request.record(now))
        return result
    }

    private fun markFailed(result: PushRegistrationResult): PushRegistrationResult {
        failed.value = true
        return result
    }

    private fun unexpected(failure: Throwable): Outcome<Unit> =
        Outcome.Failure(RemoteFailure.ProtocolFailure(failure.javaClass.simpleName))
}
