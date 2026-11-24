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
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.update.UpdatePhase
import dev.proflyder.home.feature.update.UpdateState
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal enum class SettingsUpdateStatus {
    IDLE,
    CHECKING,
    UP_TO_DATE,
    AVAILABLE,
    SIGN_IN_REQUIRED,
    DOWNLOADING,
    INSTALLING,
    OFFLINE,
    FAILED,
}

internal data class SettingsUpdateRow(
    val visible: Boolean = false,
    val version: String = "",
    val latestVersion: String = "",
    val status: SettingsUpdateStatus = SettingsUpdateStatus.IDLE,
)

internal data class SettingsUiState(
    val designSystemVisible: Boolean = false,
    val links: Set<SettingsLink> = SettingsLink.entries.toSet(),
    val update: SettingsUpdateRow = SettingsUpdateRow(),
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

    data object CheckUpdates : SettingsIntent
}

internal sealed interface SettingsEvent {
    data class OpenRoute(val route: Route) : SettingsEvent
}

@Inject
internal class SettingsViewModel(
    version: AppVersion,
    access: AppAccessSource,
    private val updates: UpdateCenter,
) : StateViewModel<SettingsUiState, SettingsIntent, SettingsEvent>(
    SettingsUiState(
        designSystemVisible = version.debuggable,
        update = SettingsUpdateRow(visible = updates.supported, version = version.name),
    ),
) {
    init {
        access.state.onEach(::applyAccess).launchIn(viewModelScope)
        updates.state.onEach(::applyUpdate).launchIn(viewModelScope)
    }

    override fun handle(intent: SettingsIntent) {
        when (intent) {
            is SettingsIntent.Open -> emit(SettingsEvent.OpenRoute(intent.link.route))
            SettingsIntent.CheckUpdates -> updates.checkNow()
        }
    }

    private fun applyAccess(access: AppAccessState) {
        val links = SettingsLink.entries.filter { access.allows(it.module) }.toSet()
        updateState { copy(links = links) }
    }

    private fun applyUpdate(state: UpdateState) {
        updateState {
            copy(
                update = SettingsUpdateRow(
                    visible = updates.supported,
                    version = state.currentVersion,
                    latestVersion = state.available?.versionName.orEmpty(),
                    status = statusOf(state),
                ),
            )
        }
    }

    private fun statusOf(state: UpdateState): SettingsUpdateStatus = when (state.phase) {
        UpdatePhase.CHECKING -> SettingsUpdateStatus.CHECKING
        UpdatePhase.UP_TO_DATE -> SettingsUpdateStatus.UP_TO_DATE
        UpdatePhase.AVAILABLE -> SettingsUpdateStatus.AVAILABLE
        UpdatePhase.SIGN_IN_REQUIRED, UpdatePhase.SIGNING_IN -> SettingsUpdateStatus.SIGN_IN_REQUIRED
        UpdatePhase.DOWNLOADING -> SettingsUpdateStatus.DOWNLOADING
        UpdatePhase.INSTALLING -> SettingsUpdateStatus.INSTALLING
        UpdatePhase.FAILED -> failedStatus(state.failure)
        UpdatePhase.IDLE -> SettingsUpdateStatus.IDLE
    }

    private fun failedStatus(failure: UpdateFailureKind?): SettingsUpdateStatus =
        if (failure == UpdateFailureKind.OFFLINE) SettingsUpdateStatus.OFFLINE else SettingsUpdateStatus.FAILED
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SettingsViewModel::class)
@Inject
class SettingsViewModelSource internal constructor(
    private val viewModel: () -> SettingsViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
