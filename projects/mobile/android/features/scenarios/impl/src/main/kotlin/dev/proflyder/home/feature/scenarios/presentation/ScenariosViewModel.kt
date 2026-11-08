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
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenarios
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import java.util.UUID
import kotlinx.coroutines.Job

data class ScenariosUiState(
    val refreshing: Boolean = true,
    val scenarios: List<Scenario> = emptyList(),
    val loadError: RemoteFailure? = null,
    val actionError: RemoteFailure? = null,
    /** True while any single server mutation is in flight. */
    val busy: Boolean = false,
    /** Label of the scenario whose manual run was just queued. */
    val queuedLabel: String? = null,
    /** Ordered pinned scenario/device refs, so the list can offer Pin/Unpin. */
    val favorites: List<FavoriteRef> = emptyList(),
    val favoriteNotice: FavoriteNotice? = null,
)

data class FavoriteNotice(val label: String, val pinned: Boolean)

class ScenariosViewModel(
    private val loadScenarios: LoadScenarios,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val deleteScenario: DeleteScenario,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
) : BaseViewModel<ScenariosUiState>(ScenariosUiState()) {
    private var refreshJob: Job? = null
    private var generation = 0

    /** [silent] keeps already loaded data on screen without a spinner (tab returns, resume). */
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

    /**
     * One tap is one logical run, so the key is generated per tap. A transport
     * retry of the same tap would reuse it; a second tap is a new run.
     */
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

    /** Pins or unpins a native scenario from a long press in the list. */
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

@ContributesIntoSet(AppScope::class)
@Inject
class ScenariosViewModelFactoryContributor(
    private val loadScenarios: LoadScenarios,
    private val setScenarioEnabled: SetScenarioEnabled,
    private val deleteScenario: DeleteScenario,
    private val runScenario: RunScenario,
    private val favorites: FavoritePreferences,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = ScenariosViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        ScenariosViewModel(loadScenarios, setScenarioEnabled, deleteScenario, runScenario, favorites)
    }
}
