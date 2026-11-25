package dev.proflyder.home.feature.hermes.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.hermes.HermesPanel
import dev.proflyder.home.feature.hermes.HermesRepository
import dev.proflyder.home.feature.hermes.HermesState
import dev.proflyder.home.feature.hermes.Loaded
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold

internal data class HermesUiState(val hermes: HermesState = HermesState())

internal sealed interface HermesIntent {
    data class Refresh(val panel: HermesPanel) : HermesIntent
    data class Open(val destination: Destination) : HermesIntent
}

internal sealed interface HermesEvent {
    data class RefreshFailed(val failure: RemoteFailure) : HermesEvent
    data class OpenRoute(val route: Route) : HermesEvent
}

internal fun HermesState.of(panel: HermesPanel): Loaded<*> = when (panel) {
    HermesPanel.AGENTS -> agents
    HermesPanel.REPORTS -> reports
    HermesPanel.VOICE -> voice
}

@Inject
internal class HermesViewModel(
    private val repository: HermesRepository,
) : StateViewModel<HermesUiState, HermesIntent, HermesEvent>(HermesUiState(repository.state.value)) {
    init {
        repository.state
            .runningFold(repository.state.value to repository.state.value) { (_, previous), next -> previous to next }
            .onEach { (previous, next) -> publish(previous, next) }
            .launchIn(viewModelScope)
    }

    override fun handle(intent: HermesIntent) {
        when (intent) {
            is HermesIntent.Refresh -> repository.refresh(intent.panel)
            is HermesIntent.Open -> emit(HermesEvent.OpenRoute(Route.Screen(intent.destination)))
        }
    }

    private fun publish(previous: HermesState, next: HermesState) {
        updateState { copy(hermes = next) }
        HermesPanel.entries
            .mapNotNull { panel -> failedRefresh(previous.of(panel), next.of(panel)) }
            .forEach { emit(HermesEvent.RefreshFailed(it)) }
    }

    private fun failedRefresh(previous: Loaded<*>, next: Loaded<*>): RemoteFailure? =
        next.failure?.takeIf { previous.loading && !next.loading && next.value != null }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(HermesViewModel::class)
@Inject
class HermesViewModelSource internal constructor(
    private val viewModel: () -> HermesViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
