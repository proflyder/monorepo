package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.settings.AppearanceDestination
import dev.proflyder.home.feature.settings.DesignSystemDestination
import dev.proflyder.home.feature.settings.NotificationHistoryDestination
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal data class SettingsUiState(val designSystemVisible: Boolean = false)

internal enum class SettingsLink(val route: Route) {
    CONNECTION(Route.Tab("connection")),
    VPN(Route.Tab("vpn")),
    NOTIFICATIONS(Route.Tab("push")),
    HISTORY(Route.Screen(NotificationHistoryDestination)),
    APPEARANCE(Route.Screen(AppearanceDestination)),
    DESIGN_SYSTEM(Route.Screen(DesignSystemDestination)),
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
) : StateViewModel<SettingsUiState, SettingsIntent, SettingsEvent>(SettingsUiState(version.debuggable)) {
    override fun handle(intent: SettingsIntent) {
        when (intent) {
            is SettingsIntent.Open -> emit(SettingsEvent.OpenRoute(intent.link.route))
        }
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
