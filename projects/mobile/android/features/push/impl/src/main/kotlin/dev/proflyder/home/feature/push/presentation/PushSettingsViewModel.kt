package dev.proflyder.home.feature.push.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.push.PushPermissionProbe
import dev.proflyder.home.feature.push.PushPermissionStatus
import dev.proflyder.home.feature.push.PushRegistrar
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.PushRegistrationResult
import dev.proflyder.home.feature.push.PushRegistrationStore
import dev.proflyder.home.feature.push.PushSyncStatus
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class PushSettingsUiState(
    val permissions: PushPermissionStatus = PushPermissionStatus(),
    val registration: PushRegistration = PushRegistration(),
    val sync: PushSyncStatus = PushSyncStatus(),
    val deviceName: String = "",
) {
    val canRing: Boolean get() = permissions.canRing
}

internal sealed interface PushSettingsIntent {
    data object Register : PushSettingsIntent
    data class OpenSettings(val screen: PushSettingsScreen) : PushSettingsIntent
}

internal sealed interface PushSettingsEvent {
    data object Registered : PushSettingsEvent
    data class Failed(val result: PushRegistrationResult) : PushSettingsEvent
    data class OpenSettings(val screen: PushSettingsScreen) : PushSettingsEvent
}

@Inject
internal class PushSettingsViewModel(
    private val registrar: PushRegistrar,
    private val probe: PushPermissionProbe,
    store: PushRegistrationStore,
    access: AppAccessSource,
) : StateViewModel<PushSettingsUiState, PushSettingsIntent, PushSettingsEvent>(PushSettingsUiState()) {
    init {
        combine(store.registration, registrar.status) { registration, sync -> registration to sync }
            .onEach { (registration, sync) -> updateState { copy(registration = registration, sync = sync) } }
            .launchIn(viewModelScope)
        access.state.onEach { updateState { copy(deviceName = it.tokenName()) } }.launchIn(viewModelScope)
        visibility.shown.onEach { updateState { copy(permissions = probe.status()) } }.launchIn(viewModelScope)
    }

    override fun handle(intent: PushSettingsIntent) {
        when (intent) {
            PushSettingsIntent.Register -> register()
            is PushSettingsIntent.OpenSettings -> emit(PushSettingsEvent.OpenSettings(intent.screen))
        }
    }

    private fun register() {
        launch { emit(registrar.registerNow().toEvent(PushSettingsEvent.Registered)) }
    }

    private fun AppAccessState.tokenName(): String = (this as? AppAccessState.Ready)?.access?.name.orEmpty()

    private fun PushRegistrationResult.toEvent(success: PushSettingsEvent): PushSettingsEvent = when (this) {
        PushRegistrationResult.Done -> success
        else -> PushSettingsEvent.Failed(this)
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(PushSettingsViewModel::class)
@Inject
class PushSettingsViewModelSource internal constructor(
    private val viewModel: () -> PushSettingsViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
