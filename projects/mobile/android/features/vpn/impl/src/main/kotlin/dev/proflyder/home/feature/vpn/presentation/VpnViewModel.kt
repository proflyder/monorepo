package dev.proflyder.home.feature.vpn.presentation

import android.content.Intent
import android.net.Uri
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.ui.BaseViewModel
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
}

data class VpnUiState(
    val connection: VpnConnection = VpnConnection.Disconnected,
    val profile: VpnProfileState = VpnProfileState.Missing,
    val settings: VpnSettings = VpnSettings(),
    val atHome: Boolean? = null,
    val externalInstalled: Boolean = false,
    val busy: Boolean = false,
    val notice: VpnNotice? = null,
)

class VpnViewModel(
    private val controller: TunnelVpnController,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val home: HomeNetworkMonitor,
    private val certificates: VpnCertificateStore,
    private val external: ExternalOpenConnectTransport,
) : BaseViewModel<VpnUiState>(VpnUiState()) {
    init {
        launch { observe() }
    }

    private suspend fun observe() {
        combine(controller.connection, profiles.state, settings.settings, home.atHome) { connection, profile, value, atHome ->
            currentState.copy(connection = connection, profile = profile, settings = value, atHome = atHome)
        }.collect { next -> updateState { next.copy(busy = busy, notice = notice) } }
    }

    fun refresh() {
        updateState { copy(externalInstalled = external.clientInstalled()) }
        home.refresh()
    }

    fun permissionIntent(): Intent? = controller.permissionIntent()

    fun toggle() = when (currentState.connection) {
        is VpnConnection.Connecting, is VpnConnection.Connected, VpnConnection.FallbackRequired -> controller.disconnect()
        else -> controller.connect()
    }

    fun connect() = controller.connect()

    fun openExternal() = controller.connectExternal()

    fun setRoute(route: VpnRoute) {
        launch {
            settings.setRoute(route)
            controller.restartIfActive()
        }
    }

    fun setAutoConnect(enabled: Boolean) {
        launch {
            settings.setAutoConnect(enabled)
            home.refresh()
        }
    }

    fun setConnectOnOpen(enabled: Boolean) {
        launch { settings.setConnectOnOpen(enabled) }
    }

    fun setAuth(auth: VpnAuth) {
        launch { settings.setAuth(auth) }
    }

    fun reissue() = busy { reissueNow() }

    fun forget() = busy { forgetNow() }

    fun importCertificate(uri: Uri, password: String) = busy { notify(certificateNotice(certificates.import(uri, password))) }

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
        updateState { copy(notice = notice) }
    }

    suspend fun installIntent(): Intent? = certificates.installIntent()

    fun consumeNotice() {
        updateState { copy(notice = null) }
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

    override fun createFactory() = SimpleViewModelFactory {
        VpnViewModel(controller, profiles, settings, home, certificates, external)
    }
}
