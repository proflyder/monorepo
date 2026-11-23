package dev.proflyder.home.feature.timers.presentation

import android.content.Context
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
import dev.proflyder.home.core.presentation.ExclusiveActions
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.data.backgroundFailures
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.feature.timers.di.TimersFeatureScope
import dev.proflyder.home.feature.timers.di.TimersGraph
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.timers.TimerRepository
import dev.proflyder.home.feature.timers.domain.StartTimer
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.toTimerInput
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class TimersUiState(
    val load: ListLoadState = ListLoadState(),
    val timers: List<Timer> = emptyList(),
    val rows: List<TimerRow> = emptyList(),
    val form: TimerForm = TimerForm(),
    val starting: Boolean = false,
    val busyId: String? = null,
)

internal sealed interface TimersIntent {
    data class Refresh(val mode: RefreshMode) : TimersIntent
    data class SetLabel(val value: String) : TimersIntent
    data class SetDuration(val value: String) : TimersIntent
    data class Preset(val duration: Duration) : TimersIntent
    data object Start : TimersIntent
    data class TogglePause(val id: String) : TimersIntent
    data class RequestCancel(val id: String) : TimersIntent
    data class Cancel(val id: String) : TimersIntent
}

internal sealed interface TimersEvent {
    data class Failure(val failure: RemoteFailure) : TimersEvent
    data class ConfirmCancel(val id: String) : TimersEvent
}

internal class TimersViewModel(
    private val startTimer: StartTimer,
    private val repository: TimerRepository,
    private val serverClock: ServerClock,
    private val settings: ConnectionSettingsStore,
    private val defaultLabel: String,
) : StateViewModel<TimersUiState, TimersIntent, TimersEvent>(TimersUiState()) {
    private val actions = ExclusiveActions(
        scope = viewModelScope,
        onBusy = { id -> updateState { copy(busyId = id) } },
        onFailure = { emit(TimersEvent.Failure(it)) },
    )
    private val startIdentity = IdempotentRequestKey<StartPayload>()

    private data class StartPayload(val label: String, val seconds: Long, val endpoint: String)

    init {
        combine(repository.items, repository.load, ::onStore).launchIn(viewModelScope)
        repository.backgroundFailures().onEach { emit(TimersEvent.Failure(it)) }.launchIn(viewModelScope)
        countdownWhileVisible()
    }

    override fun handle(intent: TimersIntent) {
        when (intent) {
            is TimersIntent.Refresh -> refresh(intent.mode)
            is TimersIntent.SetLabel -> updateState { copy(form = form.copy(label = intent.value)) }
            is TimersIntent.SetDuration -> updateState { copy(form = form.copy(duration = intent.value, durationInvalid = false)) }
            is TimersIntent.Preset -> updateState { copy(form = form.copy(duration = intent.duration.toTimerInput(), durationInvalid = false)) }
            TimersIntent.Start -> start()
            is TimersIntent.TogglePause -> togglePause(intent.id)
            is TimersIntent.RequestCancel -> emit(TimersEvent.ConfirmCancel(intent.id))
            is TimersIntent.Cancel -> cancel(intent.id)
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun countdownWhileVisible() {
        visibility.visible
            .flatMapLatest { visible -> if (visible) tickerFlow(TICK) else emptyFlow() }
            .onEach { tick() }
            .launchIn(viewModelScope)
    }

    private fun tick() {
        updateState { copy(rows = timers.toRows()) }
    }

    private fun refresh(mode: RefreshMode) {
        repository.refresh(mode)
    }

    private fun onStore(timers: List<Timer>, load: ListLoadState) = updateState { withTimers(timers).copy(load = load) }

    private fun start() {
        val form = currentState.form.validated()
        val seconds = form.seconds
        updateState { copy(form = form) }
        if (seconds == null || currentState.starting) return
        val label = form.labelOr(defaultLabel)
        val key = startIdentity.keyFor(StartPayload(label, seconds, settings.config.value.endpoint?.baseUrl().orEmpty()))
        updateState { copy(starting = true) }
        launch { started(startTimer(label, seconds, key)) }
    }

    private fun started(result: Outcome<Timer>) {
        when (result) {
            is Outcome.Success -> startIdentity.markSucceeded()
            is Outcome.Failure -> if (!result.failure.isAmbiguousRetryable()) startIdentity.reset()
        }
        updateState { copy(starting = false) }
        if (result is Outcome.Failure) emit(TimersEvent.Failure(result.failure))
    }

    private fun togglePause(id: String) {
        val timer = currentState.timers.firstOrNull { it.id == id } ?: return
        val paused = timer.state == TimerState.PAUSED
        actions.run(id, { if (paused) repository.resume(id) else repository.pause(id) }) { }
    }

    private fun cancel(id: String) {
        actions.run(id, { repository.cancel(id) }) { }
    }

    private fun TimersUiState.withTimers(next: List<Timer>): TimersUiState = copy(timers = next, rows = next.toRows())

    private fun List<Timer>.toRows(): List<TimerRow> = map { TimerRow(it, serverClock.remainingSeconds(it)) }

    private companion object {
        val TICK = 1.seconds
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(TimersViewModel::class)
@Inject
class TimersViewModelSource internal constructor(
    private val scope: TimersFeatureScope,
    private val serverClock: ServerClock,
    private val settings: ConnectionSettingsStore,
    private val context: Context,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel(::build)

    private fun build(graph: TimersGraph) = TimersViewModel(
        startTimer = graph.startTimer,
        repository = graph.repository,
        serverClock = serverClock,
        settings = settings,
        defaultLabel = context.getString(R.string.timers_default_label),
    )
}
