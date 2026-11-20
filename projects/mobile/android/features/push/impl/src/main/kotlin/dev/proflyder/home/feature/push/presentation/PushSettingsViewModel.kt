package dev.proflyder.home.feature.push.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.push.PushPermissionStatus
import dev.proflyder.home.feature.push.PushRegistration
import dev.proflyder.home.feature.push.PushRegistrar
import dev.proflyder.home.feature.push.PushRegistrationStore
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

enum class PushNotice {
    REGISTERED,
    NAME_SAVED,
    FAILED,
}

data class PushSettingsUiState(
    val permissions: PushPermissionStatus = PushPermissionStatus(),
    val registration: PushRegistration = PushRegistration(),
    val deviceName: String = "",
    val notice: PushNotice? = null,
) {
    val canRing: Boolean get() = permissions.canRing
}

class PushSettingsViewModel(
    private val registrar: PushRegistrar,
    private val store: PushRegistrationStore,
) : BaseViewModel<PushSettingsUiState>(
    PushSettingsUiState(
        permissions = registrar.permissions(),
        registration = store.registration.value,
        deviceName = store.registration.value.deviceName,
    ),
) {
    init {
        launch {
            store.registration.collect { registration ->
                updateState {
                    copy(registration = registration, deviceName = registration.deviceName)
                }
            }
        }
    }

    fun refreshPermissions() {
        updateState { copy(permissions = registrar.permissions()) }
    }

    fun registerNow() {
        launch {
            registrar.registerNow()
            val failed = store.registration.value.failed
            updateState { copy(notice = if (failed) PushNotice.FAILED else PushNotice.REGISTERED) }
        }
    }

    fun saveDeviceName(name: String) {
        launch {
            val saved = registrar.saveDeviceName(name)
            val notice = if (saved) PushNotice.NAME_SAVED else PushNotice.FAILED
            updateState { copy(notice = notice) }
        }
    }

    fun consumeNotice() {
        updateState { copy(notice = null) }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class PushSettingsViewModelFactoryContributor(
    private val registrar: PushRegistrar,
    private val store: PushRegistrationStore,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = PushSettingsViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        PushSettingsViewModel(registrar, store)
    }
}
