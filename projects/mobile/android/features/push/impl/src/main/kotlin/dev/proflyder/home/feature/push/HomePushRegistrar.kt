package dev.proflyder.home.feature.push

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.operation.suspendRunCatching
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
    private val identity: DeviceIdentity,
    private val settings: ConnectionSettingsStore,
    private val version: AppVersion,
    private val clock: Clock,
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
            wanted().collectLatest { register(it, force = false) }
        }
    }

    override suspend fun registerNow(): Boolean {
        val request = current()
        return request != null && register(request, force = true)
    }

    override suspend fun saveDeviceName(name: String): Boolean {
        store.savedName(PushRegistrationPolicy.cleanName(name, identity.defaultName()))
        val request = current()
        return request == null || register(request, force = false)
    }

    internal fun unregister(token: String) {
        scope.launch { inFlight.run { suspendRunCatching { publisher.unregister(token) } } }
    }

    private fun wanted(): Flow<PushRegistrationRequest> = combine(
        tokens.token.filterNotNull(),
        settings.config.map(PushRegistrationPolicy::serverOf),
        store.registration.map { it.deviceName },
    ) { token, server, name -> request(token, server, name) }
        .filterNotNull()
        .distinctUntilChanged()

    private fun current(): PushRegistrationRequest? = request(
        token = tokens.token.value,
        server = PushRegistrationPolicy.serverOf(settings.config.value),
        rawName = store.registration.value.deviceName,
    )

    private fun request(token: String?, server: String?, rawName: String): PushRegistrationRequest? {
        if (token.isNullOrBlank() || server == null) return null
        val name = PushRegistrationPolicy.cleanName(rawName, identity.defaultName())
        val appVersion = PushRegistrationPolicy.cleanName(version.name, "")
        return PushRegistrationRequest(token, name, appVersion, server)
    }

    private suspend fun register(request: PushRegistrationRequest, force: Boolean): Boolean =
        registering.withLock { registerLocked(request, force) }

    private suspend fun registerLocked(request: PushRegistrationRequest, force: Boolean): Boolean {
        val now = clock.wallMillis()
        if (!force && !PushRegistrationPolicy.isDue(store.registration.value, request, now)) return true
        val published = inFlight.run { suspendRunCatching { publisher.publish(request) }.getOrNull() }
        val ok = published is Outcome.Success
        failed.value = !ok
        if (ok) store.saved(request.record(now))
        return ok
    }
}
