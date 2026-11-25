package dev.proflyder.home.feature.system.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.administrator
import dev.proflyder.home.core.access.hermesAllowed
import dev.proflyder.home.core.access.secretsAllowed
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.system.SystemDetailDestination
import dev.proflyder.home.feature.system.SystemRepository
import dev.proflyder.home.feature.system.SystemState
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold

internal data class SystemUiState(
    val snapshot: SystemState = SystemState(),
    val links: SystemLinks = SystemLinks(),
) {
    val loading: Boolean get() = snapshot.loading

    val error: RemoteFailure? get() = snapshot.error
}

internal sealed interface SystemIntent {
    data object Refresh : SystemIntent
    data class OpenSection(val section: SystemSection) : SystemIntent
    data class OpenLink(val route: Route) : SystemIntent
}

internal sealed interface SystemEvent {
    data class RefreshFailed(val failure: RemoteFailure) : SystemEvent
    data class OpenRoute(val route: Route) : SystemEvent
}

@Inject
internal class SystemViewModel(
    private val repository: SystemRepository,
    access: AppAccessSource,
) : StateViewModel<SystemUiState, SystemIntent, SystemEvent>(SystemUiState()) {
    init {
        combine(repository.state, access.state) { state, accessState ->
            SystemUiState(
                snapshot = state,
                links = SystemLinks(accessState.administrator, accessState.secretsAllowed, accessState.hermesAllowed),
            )
        }
            .runningFold(state.value to state.value) { (_, previous), next -> previous to next }
            .onEach { (previous, next) -> publish(previous, next) }
            .launchIn(viewModelScope)
    }

    override fun handle(intent: SystemIntent) {
        when (intent) {
            SystemIntent.Refresh -> repository.refresh()
            is SystemIntent.OpenSection ->
                emit(SystemEvent.OpenRoute(Route.Screen(SystemDetailDestination(intent.section))))
            is SystemIntent.OpenLink -> emit(SystemEvent.OpenRoute(intent.route))
        }
    }

    private fun publish(previous: SystemUiState, next: SystemUiState) {
        updateState { next }
        val finished = previous.loading && !next.loading
        val failure = next.error?.takeIf { next.snapshot.snapshot != null }
        if (finished && failure != null) emit(SystemEvent.RefreshFailed(failure))
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SystemViewModel::class)
@Inject
class SystemViewModelSource internal constructor(
    private val viewModel: () -> SystemViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
