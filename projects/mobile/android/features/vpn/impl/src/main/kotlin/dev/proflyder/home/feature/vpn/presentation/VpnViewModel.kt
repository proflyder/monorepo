package dev.proflyder.home.feature.vpn.presentation

import android.content.Intent
import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.vpn.VpnAuth
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.data.CertificateImport
import dev.proflyder.home.feature.vpn.data.VpnCertificateStore
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.feature.vpn.tunnel.ExternalOpenConnectTransport
import dev.proflyder.home.feature.vpn.tunnel.HomeNetworkMonitor
import dev.proflyder.home.feature.vpn.tunnel.TunnelVpnController
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.flow.combine

enum class VpnNotice {
    REISSUED,
    FORGOTTEN,
    CERTIFICATE_IMPORTED,
    CERTIFICATE_WRONG_PASSWORD,
    CERTIFICATE_UNREADABLE,
    CERTIFICATE_TOO_LARGE,
    PERMISSION_DENIED,
}

internal sealed interface VpnIntent {
    data object Refresh : VpnIntent
    data object ToggleConnection : VpnIntent
    data class PermissionResult(val granted: Boolean) : VpnIntent
    data object OpenExternal : VpnIntent
    data class SetRoute(val route: VpnRoute) : VpnIntent
    data class SetAuth(val auth: VpnAuth) : VpnIntent
    data class SetAutoConnect(val enabled: Boolean) : VpnIntent
    data class SetConnectOnOpen(val enabled: Boolean) : VpnIntent
    data object Reissue : VpnIntent
    data object Forget : VpnIntent
    data class ImportCertificate(val uri: Uri, val password: String) : VpnIntent
    data object InstallCertificate : VpnIntent
}

internal sealed interface VpnEvent {
    data class Notice(val notice: VpnNotice) : VpnEvent
    data class RequestPermission(val request: Intent) : VpnEvent
    data class Open(val target: Intent) : VpnEvent
}

data class VpnUiState(
    val connection: VpnConnection = VpnConnection.Disconnected,
    val profile: VpnProfileState = VpnProfileState.Missing,
    val settings: VpnSettings = VpnSettings(),
    val atHome: Boolean? = null,
    val externalInstalled: Boolean = false,
    val busy: Boolean = false,
)

internal class VpnViewModel(
    private val controller: TunnelVpnController,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val home: HomeNetworkMonitor,
    private val certificates: VpnCertificateStore,
    private val external: ExternalOpenConnectTransport,
) : StateViewModel<VpnUiState, VpnIntent, VpnEvent>(VpnUiState()) {
    init {
        launch { observe() }
    }

    private suspend fun observe() {
        combine(controller.connection, profiles.state, settings.settings, home.atHome) { connection, profile, value, atHome ->
            currentState.copy(connection = connection, profile = profile, settings = value, atHome = atHome)
        }.collect { next -> updateState { next.copy(busy = busy) } }
    }

    override fun handle(intent: VpnIntent) {
        when (intent) {
            VpnIntent.Refresh -> refresh()
            VpnIntent.ToggleConnection -> toggle()
            is VpnIntent.PermissionResult -> onPermission(intent.granted)
            VpnIntent.OpenExternal -> controller.connectExternal()
            is VpnIntent.SetRoute -> setRoute(intent.route)
            is VpnIntent.SetAuth -> launchSetting { settings.setAuth(intent.auth) }
            is VpnIntent.SetAutoConnect -> setAutoConnect(intent.enabled)
            is VpnIntent.SetConnectOnOpen -> launchSetting { settings.setConnectOnOpen(intent.enabled) }
            VpnIntent.Reissue -> busy { reissueNow() }
            VpnIntent.Forget -> busy { forgetNow() }
            is VpnIntent.ImportCertificate -> busy { notify(certificateNotice(certificates.import(intent.uri, intent.password))) }
            VpnIntent.InstallCertificate -> installCertificate()
        }
    }

    private fun refresh() {
        updateState { copy(externalInstalled = external.clientInstalled()) }
        home.refresh()
    }

    private fun toggle() {
        val request = controller.permissionIntent()
        when {
            currentState.connection.isActive -> controller.disconnect()
            request != null -> emit(VpnEvent.RequestPermission(request))
            else -> controller.connect()
        }
    }

    private fun onPermission(granted: Boolean) {
        when (granted) {
            true -> controller.connect()
            false -> notify(VpnNotice.PERMISSION_DENIED)
        }
    }

    private fun setRoute(route: VpnRoute) = launchSetting {
        settings.setRoute(route)
        controller.restartIfActive()
    }

    private fun setAutoConnect(enabled: Boolean) = launchSetting {
        settings.setAutoConnect(enabled)
        home.refresh()
    }

    private fun installCertificate() = launchSetting {
        certificates.installIntent()?.let { emit(VpnEvent.Open(it)) }
    }

    private fun launchSetting(block: suspend () -> Unit) {
        launch { block() }
    }

    private suspend fun reissueNow() {
        when (profiles.reissue()) {
            true -> {
                controller.restartIfActive()
                notify(VpnNotice.REISSUED)
            }
            false -> Unit
        }
    }

    private suspend fun forgetNow() {
        controller.disconnect()
        when (profiles.forget()) {
            true -> {
                certificates.clear()
                notify(VpnNotice.FORGOTTEN)
            }
            false -> Unit
        }
    }

    private fun certificateNotice(result: CertificateImport): VpnNotice = when (result) {
        CertificateImport.IMPORTED -> VpnNotice.CERTIFICATE_IMPORTED
        CertificateImport.WRONG_PASSWORD -> VpnNotice.CERTIFICATE_WRONG_PASSWORD
        CertificateImport.UNREADABLE -> VpnNotice.CERTIFICATE_UNREADABLE
        CertificateImport.TOO_LARGE -> VpnNotice.CERTIFICATE_TOO_LARGE
    }

    private fun notify(notice: VpnNotice) {
        emit(VpnEvent.Notice(notice))
    }

    private fun busy(block: suspend () -> Unit) {
        launch {
            updateState { copy(busy = true) }
            block()
            updateState { copy(busy = false) }
        }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class VpnViewModelFactoryContributor(
    private val controller: TunnelVpnController,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val home: HomeNetworkMonitor,
    private val certificates: VpnCertificateStore,
    private val external: ExternalOpenConnectTransport,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = VpnViewModel::class.java

    override fun createFactory(): ViewModelProvider.Factory = SimpleViewModelFactory {
        VpnViewModel(controller, profiles, settings, home, certificates, external)
    }
}
