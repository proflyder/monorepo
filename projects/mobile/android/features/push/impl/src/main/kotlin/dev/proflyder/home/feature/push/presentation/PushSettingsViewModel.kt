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

/** One-shot confirmations the screen shows and then forgets. */
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
    /** The loud path is only honest when every permission is granted. */
    val canRing: Boolean get() = permissions.canRing
}

/**
 * Reads the real permission state on every resume (the owner changes it in
 * system settings, not in the app) and triggers a token re-registration.
 */
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

    /** Re-reads the permissions; called from `onResume`. */
    fun refreshPermissions() {
        updateState { copy(permissions = registrar.permissions()) }
    }

    /** Asks for a fresh registration and reports the outcome once. */
    fun registerNow() {
        launch {
            registrar.registerNow()
            val failed = store.registration.value.failed
            updateState { copy(notice = if (failed) PushNotice.FAILED else PushNotice.REGISTERED) }
        }
    }

    /** Stores the name locally and syncs it to the server without a token re-registration. */
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
