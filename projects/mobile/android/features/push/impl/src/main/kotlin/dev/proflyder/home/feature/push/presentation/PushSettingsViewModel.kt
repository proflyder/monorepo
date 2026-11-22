package dev.proflyder.home.feature.push.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.push.PushPermissionProbe
import dev.proflyder.home.feature.push.PushPermissionStatus
import dev.proflyder.home.feature.push.PushRegistrar
import dev.proflyder.home.feature.push.PushRegistration
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
    val nameDraft: String? = null,
) {
    val canRing: Boolean get() = permissions.canRing
    val deviceName: String get() = nameDraft ?: registration.deviceName
}

internal sealed interface PushSettingsIntent {
    data object Register : PushSettingsIntent
    data class EditName(val name: String) : PushSettingsIntent
    data object SaveName : PushSettingsIntent
    data class OpenSettings(val screen: PushSettingsScreen) : PushSettingsIntent
}

internal sealed interface PushSettingsEvent {
    data object Registered : PushSettingsEvent
    data object NameSaved : PushSettingsEvent
    data object Failed : PushSettingsEvent
    data class OpenSettings(val screen: PushSettingsScreen) : PushSettingsEvent
}

@Inject
internal class PushSettingsViewModel(
    private val registrar: PushRegistrar,
    private val probe: PushPermissionProbe,
    store: PushRegistrationStore,
) : StateViewModel<PushSettingsUiState, PushSettingsIntent, PushSettingsEvent>(PushSettingsUiState()) {
    init {
        combine(store.registration, registrar.status) { registration, sync -> registration to sync }
            .onEach { (registration, sync) -> updateState { copy(registration = registration, sync = sync) } }
            .launchIn(viewModelScope)
        screenShown.onEach { updateState { copy(permissions = probe.status()) } }.launchIn(viewModelScope)
    }

    override fun handle(intent: PushSettingsIntent) {
        when (intent) {
            PushSettingsIntent.Register -> register()
            is PushSettingsIntent.EditName -> updateState { copy(nameDraft = intent.name) }
            PushSettingsIntent.SaveName -> saveName()
            is PushSettingsIntent.OpenSettings -> emit(PushSettingsEvent.OpenSettings(intent.screen))
        }
    }

    private fun register() {
        launch {
            emit(if (registrar.registerNow()) PushSettingsEvent.Registered else PushSettingsEvent.Failed)
        }
    }

    private fun saveName() {
        val name = state.value.deviceName
        launch {
            val saved = registrar.saveDeviceName(name)
            updateState { copy(nameDraft = null) }
            emit(if (saved) PushSettingsEvent.NameSaved else PushSettingsEvent.Failed)
        }
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
