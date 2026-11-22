package dev.proflyder.home.feature.scenarios.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.presentation.AppEventBus
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.eventsOf
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.scenarios.domain.LoadScenarios
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.util.UUID
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class ScenariosUiState(
    val refreshing: Boolean = true,
    val scenarios: List<Scenario> = emptyList(),
    val loadError: RemoteFailure? = null,
    val busy: Boolean = false,
    val favorites: List<FavoriteRef> = emptyList(),
) {
    fun isPinned(scenario: Scenario): Boolean = FavoriteRef(FavoriteKind.SCENARIO, scenario.id) in favorites
}

internal sealed interface ScenariosIntent {
    data class Refresh(val mode: RefreshMode) : ScenariosIntent
    data class SetEnabled(val scenario: Scenario, val enabled: Boolean) : ScenariosIntent
    data class Run(val scenario: Scenario) : ScenariosIntent
    data class Open(val scenario: Scenario) : ScenariosIntent
    data class AskPin(val scenario: Scenario) : ScenariosIntent
    data class TogglePin(val scenario: Scenario) : ScenariosIntent
}

internal sealed interface ScenariosEvent {
    data class Failure(val failure: RemoteFailure) : ScenariosEvent
    data class Queued(val label: String) : ScenariosEvent
    data class Pinned(val label: String, val pinned: Boolean) : ScenariosEvent
    data class Open(val scenarioId: String) : ScenariosEvent
    data class ConfirmPin(val scenario: Scenario, val pinned: Boolean) : ScenariosEvent
}

internal class ScenariosViewModel(
    private val loadScenarios: LoadScenarios,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
    events: AppEventBus,
) : StateViewModel<ScenariosUiState, ScenariosIntent, ScenariosEvent>(ScenariosUiState()) {
    private val loads by latest()

    init {
        refresh(RefreshMode.HARD)
        favorites.favorites.onEach { stored -> updateState { copy(favorites = stored.orEmpty()) } }.launchIn(viewModelScope)
        events.eventsOf<ScenariosChanged>().onEach { refresh(RefreshMode.SOFT) }.launchIn(viewModelScope)
    }

    override fun handle(intent: ScenariosIntent) {
        when (intent) {
            is ScenariosIntent.Refresh -> refresh(intent.mode)
            is ScenariosIntent.SetEnabled -> setEnabled(intent.scenario, intent.enabled)
            is ScenariosIntent.Run -> run(intent.scenario)
            is ScenariosIntent.Open -> emit(ScenariosEvent.Open(intent.scenario.id))
            is ScenariosIntent.AskPin -> emit(ScenariosEvent.ConfirmPin(intent.scenario, currentState.isPinned(intent.scenario)))
            is ScenariosIntent.TogglePin -> togglePin(intent.scenario)
        }
    }

    private fun refresh(mode: RefreshMode) {
        updateState { copy(refreshing = refreshing || mode == RefreshMode.HARD, loadError = null) }
        loads.launch { reload() }
    }

    private suspend fun reload() {
        val result = loadScenarios()
        updateState { loaded(result) }
        if (result is Outcome.Failure && currentState.scenarios.isNotEmpty()) emit(ScenariosEvent.Failure(result.failure))
    }

    private fun ScenariosUiState.loaded(result: Outcome<List<Scenario>>): ScenariosUiState = when (result) {
        is Outcome.Success -> copy(refreshing = false, scenarios = result.value.sortedWith(ORDER), loadError = null)
        is Outcome.Failure -> copy(refreshing = false, loadError = result.failure)
    }

    private fun setEnabled(scenario: Scenario, enabled: Boolean) = act {
        when (val result = setScenarioEnabled(scenario.id, enabled)) {
            is Outcome.Success -> updateState { copy(scenarios = scenarios.replaced(result.value)) }
            is Outcome.Failure -> emit(ScenariosEvent.Failure(result.failure))
        }
    }

    private fun List<Scenario>.replaced(updated: Scenario): List<Scenario> = map { if (it.id == updated.id) updated else it }

    private fun run(scenario: Scenario) = act {
        when (val result = runScenario(scenario.id, UUID.randomUUID().toString())) {
            is Outcome.Success -> emit(ScenariosEvent.Queued(scenario.label))
            is Outcome.Failure -> emit(ScenariosEvent.Failure(result.failure))
        }
    }

    private fun act(block: suspend () -> Unit) {
        if (currentState.busy) return
        updateState { copy(busy = true) }
        launch { block().also { updateState { copy(busy = false) } } }
    }

    private fun togglePin(scenario: Scenario) {
        launch {
            val pinned = favorites.toggle(FavoriteRef(FavoriteKind.SCENARIO, scenario.id))
            emit(ScenariosEvent.Pinned(scenario.label, pinned))
        }
    }

    private companion object {
        val ORDER: Comparator<Scenario> = compareBy({ !it.enabled }, { it.label.lowercase() })
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(ScenariosViewModel::class)
@Inject
class ScenariosViewModelSource internal constructor(
    private val loadScenarios: LoadScenarios,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
    private val events: AppEventBus,
) : ViewModelSource {
    override fun create(): ViewModel = run {
        ScenariosViewModel(loadScenarios, setScenarioEnabled, runScenario, favorites, events)
    }
}
