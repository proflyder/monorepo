package dev.proflyder.home.core.access

import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.AccessApi
import dev.proflyder.home.core.network.ModuleForbiddenSink
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<AppAccessSource>())
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@ContributesIntoSet(AppScope::class, binding = binding<ModuleForbiddenSink>())
@Inject
class AppAccessSession(
    private val api: AccessApi,
    private val signals: SyncSignals,
    private val scope: CoroutineScope,
) : AppAccessSource, AppStartable, ModuleForbiddenSink {
    private val mutableState = MutableStateFlow<AppAccessState>(AppAccessState.Unknown)
    private val loading = AtomicBoolean(false)

    override val state: StateFlow<AppAccessState> = mutableState.asStateFlow()

    override fun start() {
        refresh()
        signals.signals
            .filterIsInstance<SyncSignal.Stale>()
            .onEach { refresh() }
            .launchIn(scope)
    }

    fun refresh() {
        if (!loading.compareAndSet(false, true)) return
        scope.launch { load() }
    }

    override fun moduleForbidden(module: String) = refresh()

    private suspend fun load() {
        val outcome = runCatching { api.me() }.getOrNull()
        loading.set(false)
        if (outcome is Outcome.Success) mutableState.value = AppAccessState.Ready(outcome.value)
    }
}
