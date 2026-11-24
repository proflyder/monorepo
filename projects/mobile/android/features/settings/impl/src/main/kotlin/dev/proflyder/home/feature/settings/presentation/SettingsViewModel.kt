package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.settings.AppearanceDestination
import dev.proflyder.home.feature.settings.DesignSystemDestination
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class SettingsUiState(
    val designSystemVisible: Boolean = false,
    val links: Set<SettingsLink> = SettingsLink.entries.toSet(),
)

internal enum class SettingsLink(val route: Route, val module: AppModule) {
    CONNECTION(Route.Tab("connection"), AppModule.BASE),
    VPN(Route.Tab("vpn"), AppModule.VPN),
    NOTIFICATIONS(Route.Tab("push"), AppModule.NOTIFICATIONS),
    APPEARANCE(Route.Screen(AppearanceDestination), AppModule.BASE),
    DESIGN_SYSTEM(Route.Screen(DesignSystemDestination), AppModule.BASE),
}

internal sealed interface SettingsIntent {
    data class Open(val link: SettingsLink) : SettingsIntent
}

internal sealed interface SettingsEvent {
    data class OpenRoute(val route: Route) : SettingsEvent
}

@Inject
internal class SettingsViewModel(
    version: AppVersion,
    access: AppAccessSource,
) : StateViewModel<SettingsUiState, SettingsIntent, SettingsEvent>(
    SettingsUiState(designSystemVisible = version.debuggable),
) {
    init {
        access.state.onEach(::applyAccess).launchIn(viewModelScope)
    }

    override fun handle(intent: SettingsIntent) {
        when (intent) {
            is SettingsIntent.Open -> emit(SettingsEvent.OpenRoute(intent.link.route))
        }
    }

    private fun applyAccess(access: AppAccessState) {
        val links = SettingsLink.entries.filter { access.allows(it.module) }.toSet()
        updateState { copy(links = links) }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SettingsViewModel::class)
@Inject
class SettingsViewModelSource internal constructor(
    private val viewModel: () -> SettingsViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
