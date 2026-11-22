package dev.proflyder.home.feature.scenarios.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenarios
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import java.util.UUID
import kotlinx.coroutines.Job

data class ScenariosUiState(
    val refreshing: Boolean = true,
    val scenarios: List<Scenario> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    val busy: Boolean = false,
    val queuedLabel: String? = null,
    val favorites: List<FavoriteRef> = emptyList(),
    val favoriteNotice: FavoriteNotice? = null,
)

data class FavoriteNotice(val label: String, val pinned: Boolean)

@Inject
internal class ScenariosViewModel(
    private val loadScenarios: LoadScenarios,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val deleteScenario: DeleteScenario,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
) : BaseViewModel<ScenariosUiState>(ScenariosUiState()) {
    private var refreshJob: Job? = null
    private var generation = 0

    fun refresh(silent: Boolean = false) {
        refreshJob?.cancel()
        val requested = ++generation
        updateState { copy(refreshing = refreshing || !silent, loadError = null) }
        refreshJob = launch {
            val savedFavorites = favorites.load()
            when (val result = loadScenarios()) {
                is Outcome.Success -> {
                    if (requested != generation) return@launch
                    updateState {
                        copy(
                            refreshing = false,
                            scenarios = result.value.sortedWith(
                                compareBy({ !it.enabled }, { it.label.lowercase() }),
                            ),
                            loadError = null,
                            favorites = savedFavorites ?: favorites,
                        )
                    }
                }
                is Outcome.Failure -> {
                    if (requested != generation) return@launch
                    updateState {
                        copy(
                            refreshing = false,
                            loadError = result.failure,
                            favorites = savedFavorites ?: favorites,
                        )
                    }
                }
            }
        }
    }

    fun setEnabled(scenario: Scenario, enabled: Boolean) {
        if (currentState.busy) return
        updateState { copy(busy = true, actionError = null) }
        launch {
            when (val result = setScenarioEnabled(scenario.id, enabled)) {
                is Outcome.Success -> updateState { copy(busy = false, scenarios = withScenario(result.value)) }
                is Outcome.Failure -> updateState { copy(busy = false, actionError = result.failure) }
            }
        }
    }

    fun run(scenario: Scenario) {
        if (currentState.busy) return
        updateState { copy(busy = true, actionError = null, queuedLabel = null) }
        launch {
            when (val result = runScenario(scenario.id, UUID.randomUUID().toString())) {
                is Outcome.Success -> updateState { copy(busy = false, queuedLabel = scenario.label) }
                is Outcome.Failure -> updateState { copy(busy = false, actionError = result.failure) }
            }
        }
    }

    fun delete(scenario: Scenario) {
        if (currentState.busy) return
        updateState { copy(busy = true, actionError = null) }
        launch {
            when (val result = deleteScenario(scenario.id)) {
                is Outcome.Success -> updateState {
                    copy(busy = false, scenarios = scenarios.filterNot { it.id == scenario.id })
                }
                is Outcome.Failure -> updateState { copy(busy = false, actionError = result.failure) }
            }
        }
    }

    fun consumeQueued() = updateState { copy(queuedLabel = null) }

    fun toggleFavorite(scenario: Scenario) {
        val ref = FavoriteRef(FavoriteKind.SCENARIO, scenario.id)
        val updated = Favorites.toggle(currentState.favorites, ref)
        val pinned = ref in updated
        updateState {
            copy(favorites = updated, favoriteNotice = FavoriteNotice(scenario.label, pinned))
        }
        launch { favorites.save(updated) }
    }

    fun consumeFavoriteNotice() = updateState { copy(favoriteNotice = null) }

    fun consumeActionError() = updateState { copy(actionError = null) }

    fun consumeLoadError() = updateState { copy(loadError = null) }

    private fun ScenariosUiState.withScenario(updated: Scenario): List<Scenario> =
        scenarios.map { if (it.id == updated.id) updated else it }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(ScenariosViewModel::class)
@Inject
class ScenariosViewModelSource internal constructor(
    private val viewModel: () -> ScenariosViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
