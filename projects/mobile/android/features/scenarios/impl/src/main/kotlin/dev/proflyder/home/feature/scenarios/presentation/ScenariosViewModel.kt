package dev.proflyder.home.feature.scenarios.presentation

import dev.proflyder.home.feature.scenarios.ScenarioEditorDestination
import dev.proflyder.home.core.navigation.Route
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.backgroundFailures
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.scenarios.di.ScenariosFeatureScope
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.util.UUID
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class ScenariosUiState(
    val refreshing: Boolean = true,
    val scenarios: List<Scenario> = emptyList(),
    val loadError: RemoteFailure? = null,
    val busy: Boolean = false,
    val favorites: List<FavoriteRef> = emptyList(),
    val rows: List<ScenarioRow> = emptyList(),
) {
    fun isPinned(scenario: Scenario): Boolean = FavoriteRef(FavoriteKind.SCENARIO, scenario.id) in favorites
}

internal fun ScenariosUiState.derivedFrom(previous: ScenariosUiState): ScenariosUiState =
    if (scenarios == previous.scenarios && busy == previous.busy) copy(rows = previous.rows) else copy(rows = scenarios.map { ScenarioRow(it, busy) })

internal sealed interface ScenariosIntent {
    data class Refresh(val mode: RefreshMode) : ScenariosIntent
    data class SetEnabled(val scenario: Scenario, val enabled: Boolean) : ScenariosIntent
    data class Run(val scenario: Scenario) : ScenariosIntent
    data class Open(val scenario: Scenario) : ScenariosIntent
    data class AskPin(val scenario: Scenario) : ScenariosIntent
    data class TogglePin(val scenario: Scenario) : ScenariosIntent
    data object Add : ScenariosIntent
    data object OpenConnection : ScenariosIntent
}

internal sealed interface ScenariosEvent {
    data class Failure(val failure: RemoteFailure) : ScenariosEvent
    data class Queued(val label: String) : ScenariosEvent
    data class Pinned(val label: String, val pinned: Boolean) : ScenariosEvent
    data class OpenRoute(val route: Route) : ScenariosEvent
    data class ConfirmPin(val scenario: Scenario, val pinned: Boolean) : ScenariosEvent
}

internal class ScenariosViewModel(
    private val repository: ScenarioRepository,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
) : StateViewModel<ScenariosUiState, ScenariosIntent, ScenariosEvent>(ScenariosUiState()) {

    override fun derive(previous: ScenariosUiState, next: ScenariosUiState): ScenariosUiState = next.derivedFrom(previous)

    init {
        combine(repository.items, repository.load, ::onStore).launchIn(viewModelScope)
        repository.backgroundFailures().onEach { emit(ScenariosEvent.Failure(it)) }.launchIn(viewModelScope)
        favorites.favorites.onEach { stored -> updateState { copy(favorites = stored.orEmpty()) } }.launchIn(viewModelScope)
    }

    override fun handle(intent: ScenariosIntent) {
        when (intent) {
            is ScenariosIntent.Refresh -> repository.refresh(intent.mode)
            is ScenariosIntent.SetEnabled -> setEnabled(intent.scenario, intent.enabled)
            is ScenariosIntent.Run -> run(intent.scenario)
            is ScenariosIntent.Open -> openEditor(intent.scenario.id)
            ScenariosIntent.Add -> openEditor(null)
            ScenariosIntent.OpenConnection -> emit(ScenariosEvent.OpenRoute(Route.Tab(CONNECTION_FEATURE)))
            is ScenariosIntent.AskPin -> emit(ScenariosEvent.ConfirmPin(intent.scenario, currentState.isPinned(intent.scenario)))
            is ScenariosIntent.TogglePin -> togglePin(intent.scenario)
        }
    }

    private fun openEditor(scenarioId: String?) {
        emit(ScenariosEvent.OpenRoute(Route.Screen(ScenarioEditorDestination(scenarioId))))
    }

    private fun onStore(scenarios: List<Scenario>, load: ListLoadState) = updateState {
        copy(refreshing = load.refreshing, scenarios = scenarios, loadError = load.failure)
    }

    private fun setEnabled(scenario: Scenario, enabled: Boolean) = act {
        val result = setScenarioEnabled(scenario.id, enabled)
        if (result is Outcome.Failure) emit(ScenariosEvent.Failure(result.failure))
    }

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

}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(ScenariosViewModel::class)
@Inject
class ScenariosViewModelSource internal constructor(
    private val scope: ScenariosFeatureScope,
    private val favorites: FavoritePreferences,
) : ViewModelSource {
    override fun create(): ViewModel = scope.viewModel { graph ->
        ScenariosViewModel(graph.repository, graph.setScenarioEnabled, graph.runScenario, favorites)
    }
}

private const val CONNECTION_FEATURE = "connection"
