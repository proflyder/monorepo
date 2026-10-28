package dev.proflyder.home.feature.timers.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.model.isAmbiguousRetryable
import dev.proflyder.home.core.operation.IdempotentRequestKey
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.timers.domain.CancelTimer
import dev.proflyder.home.feature.timers.domain.LoadTimers
import dev.proflyder.home.feature.timers.domain.PauseTimer
import dev.proflyder.home.feature.timers.domain.ResumeTimer
import dev.proflyder.home.feature.timers.domain.StartTimer
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class TimersUiState(
    val refreshing: Boolean = true,
    val starting: Boolean = false,
    val timers: List<Timer> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val busyId: String? = null,
    val tick: Long = 0,
    val serverTimeSynced: Boolean = false,
)

class TimersViewModel(
    private val loadTimers: LoadTimers,
    private val startTimer: StartTimer,
    private val pauseTimer: PauseTimer,
    private val resumeTimer: ResumeTimer,
    private val cancelTimer: CancelTimer,
    private val clock: ServerClock,
    private val settings: ConnectionSettingsStore,
) : ViewModel() {
    private val mutableState = MutableStateFlow(TimersUiState())
    val state: StateFlow<TimersUiState> = mutableState.asStateFlow()

    private var refreshJob: Job? = null
    private var generation = 0
    private var lastRefreshNanos = System.nanoTime()
    private var lastExpiryRefreshNanos = 0L

    private val createIdentity = IdempotentRequestKey<StartPayload>()

    private data class StartPayload(
        val label: String,
        val durationSeconds: Long,
        val endpoint: String,
    )

    fun refresh() {
        refreshJob?.cancel()
        val requested = ++generation
        lastRefreshNanos = System.nanoTime()
        mutableState.value = mutableState.value.copy(refreshing = true, loadError = null)
        refreshJob = viewModelScope.launch {
            val result = loadTimers()
            if (requested != generation) return@launch
            when (result) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    timers = result.value.sortedBy { it.createdUnix },
                    loadError = null,
                    serverTimeSynced = clock.isSynced,
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    refreshing = false,
                    loadError = result.failure,
                )
            }
        }
    }

    fun start(label: String, durationSeconds: Long) {
        if (mutableState.value.starting) return
        val normalizedLabel = label.trim().ifEmpty { "Timer" }
        val payload = StartPayload(
            label = normalizedLabel,
            durationSeconds = durationSeconds,
            endpoint = settings.config.value.endpoint?.baseUrl().orEmpty(),
        )
        val key = createIdentity.keyFor(payload)
        mutableState.value = mutableState.value.copy(starting = true, actionError = null)
        viewModelScope.launch {
            when (val result = startTimer(normalizedLabel, durationSeconds, key)) {
                is Outcome.Success -> {
                    createIdentity.markSucceeded()
                    mutableState.value = mutableState.value.copy(
                        starting = false,
                        timers = (mutableState.value.timers + result.value).sortedBy { it.createdUnix },
                    )
                }
                is Outcome.Failure -> {
                    if (!result.failure.isAmbiguousRetryable()) createIdentity.reset()
                    mutableState.value = mutableState.value.copy(
                        starting = false,
                        actionError = result.failure,
                    )
                }
            }
        }
    }

    fun togglePause(timer: Timer) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = timer.id, actionError = null)
        viewModelScope.launch {
            val action = if (timer.state == TimerState.PAUSED) resumeTimer(timer.id) else pauseTimer(timer.id)
            when (val result = action) {
                is Outcome.Success -> replace(result.value)
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    actionError = result.failure,
                )
            }
        }
    }

    fun cancel(timer: Timer) {
        if (mutableState.value.busyId != null) return
        mutableState.value = mutableState.value.copy(busyId = timer.id, actionError = null)
        viewModelScope.launch {
            when (val result = cancelTimer(timer.id)) {
                is Outcome.Success -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    timers = mutableState.value.timers.filterNot { it.id == timer.id },
                )
                is Outcome.Failure -> mutableState.value = mutableState.value.copy(
                    busyId = null,
                    actionError = result.failure,
                )
            }
        }
    }

    fun tick() {
        val now = System.nanoTime()
        mutableState.value = mutableState.value.copy(tick = mutableState.value.tick + 1)
        val expired = mutableState.value.timers.any {
            it.state == TimerState.RUNNING && clock.remainingSeconds(it) == 0L
        }
        val periodicDue = now - lastRefreshNanos >= PERIODIC_REFRESH_NANOS
        val expiryDue = expired && now - lastExpiryRefreshNanos >= EXPIRY_REFRESH_NANOS
        if ((periodicDue || expiryDue) && !mutableState.value.refreshing) {
            if (expired) lastExpiryRefreshNanos = now
            refresh()
        }
    }

    fun remaining(timer: Timer): Long = clock.remainingSeconds(timer)

    fun consumeActionError() {
        mutableState.value = mutableState.value.copy(actionError = null)
    }

    fun consumeLoadError() {
        mutableState.value = mutableState.value.copy(loadError = null)
    }

    private fun replace(timer: Timer) {
        mutableState.value = mutableState.value.copy(
            busyId = null,
            timers = mutableState.value.timers.map { if (it.id == timer.id) timer else it },
        )
    }

    private companion object {
        const val PERIODIC_REFRESH_NANOS = 30_000_000_000L
        const val EXPIRY_REFRESH_NANOS = 10_000_000_000L
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class TimersViewModelFactoryContributor(
    private val loadTimers: LoadTimers,
    private val startTimer: StartTimer,
    private val pauseTimer: PauseTimer,
    private val resumeTimer: ResumeTimer,
    private val cancelTimer: CancelTimer,
    private val clock: ServerClock,
    private val settings: ConnectionSettingsStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = TimersViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        TimersViewModel(loadTimers, startTimer, pauseTimer, resumeTimer, cancelTimer, clock, settings)
    }
}
