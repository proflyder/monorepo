package dev.proflyder.home.core.access

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.core.network.ModuleForbiddenSink
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

interface AccessRefresher {
    fun refresh()
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<AppAccessSource>())
@ContributesBinding(AppScope::class, binding = binding<AccessRefresher>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@ContributesIntoSet(AppScope::class, binding = binding<ModuleForbiddenSink>())
@Inject
class AppAccessSession(
    private val api: AccessApi,
    private val settings: ConnectionSettingsStore,
    private val signals: SyncSignals,
    private val clock: Clock,
    private val scope: CoroutineScope,
) : AppAccessSource, AccessRefresher, AppStartable, ModuleForbiddenSink {
    private val mutableState = MutableStateFlow<AppAccessState>(AppAccessState.Unknown)
    private val loading = AtomicBoolean(false)
    private val generation = AtomicInteger(0)
    private val lastForbiddenNanos = AtomicLong(NEVER)
    @Volatile
    private var job: Job? = null

    override val state: StateFlow<AppAccessState> = mutableState.asStateFlow()

    override fun start() {
        observeTokenChanges()
        refresh()
        signals.signals
            .filterIsInstance<SyncSignal.Stale>()
            .onEach { refresh() }
            .launchIn(scope)
    }

    override fun refresh() {
        if (!loading.compareAndSet(false, true)) return
        val current = scope.launch { load(generation.get()) }
        job = current
        current.invokeOnCompletion { if (job === current) loading.set(false) }
    }

    override fun moduleForbidden(module: String) {
        val now = clock.elapsedNanos()
        val last = lastForbiddenNanos.get()
        if (last != NEVER && now - last < FORBIDDEN_REFRESH_NANOS) return
        if (!lastForbiddenNanos.compareAndSet(last, now)) return
        refresh()
    }

    private fun observeTokenChanges() {
        settings.config
            .map { it.bearerToken }
            .distinctUntilChanged()
            .drop(1)
            .onEach(::onTokenChanged)
            .launchIn(scope)
    }

    private fun onTokenChanged(token: String?) {
        generation.incrementAndGet()
        lastForbiddenNanos.set(NEVER)
        job?.cancel()
        job = null
        loading.set(false)
        mutableState.value = AppAccessState.Unknown
        if (token != null) refresh()
    }

    private suspend fun load(loadGeneration: Int) {
        val outcome = suspendRunCatching { api.me() }.getOrNull()
        if (loadGeneration != generation.get()) return
        if (outcome is Outcome.Success) mutableState.value = AppAccessState.Ready(outcome.value)
    }

    private companion object {
        const val NEVER = Long.MIN_VALUE

        const val FORBIDDEN_REFRESH_NANOS = 3_000_000_000L
    }
}
