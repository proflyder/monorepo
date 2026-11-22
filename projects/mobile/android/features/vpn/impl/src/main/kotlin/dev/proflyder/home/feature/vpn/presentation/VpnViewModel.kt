package dev.proflyder.home.feature.vpn.presentation

import android.content.Intent
import android.net.Uri
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.vpn.VpnAuth
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.connect.UserVpnCommands
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.feature.vpn.tunnel.VpnPermission
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest

internal enum class VpnNotice {
    REISSUED,
    FORGOTTEN,
    CERTIFICATE_IMPORTED,
    CERTIFICATE_WRONG_PASSWORD,
    CERTIFICATE_UNREADABLE,
    CERTIFICATE_TOO_LARGE,
    PERMISSION_DENIED,
}

internal sealed interface VpnIntent {
    data object ToggleConnection : VpnIntent
    data class PermissionResult(val granted: Boolean) : VpnIntent
    data class ConnectVia(val transport: VpnProtocolId) : VpnIntent
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

internal data class VpnUiState(
    val connection: VpnConnection = VpnConnection.Disconnected,
    val traffic: VpnTraffic? = null,
    val nowMillis: Long = 0,
    val profile: VpnProfileState = VpnProfileState.Loading,
    val settings: VpnSettings = VpnSettings(),
    val atHome: Boolean? = null,
    val fallbackReady: Boolean = false,
    val busy: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@Inject
internal class VpnViewModel(
    private val controller: VpnController,
    private val settings: VpnSettingsStore,
    private val permission: VpnPermission,
    private val actions: VpnProfileActions,
    private val commands: UserVpnCommands,
    feed: VpnStateFeed,
) : StateViewModel<VpnUiState, VpnIntent, VpnEvent>(VpnUiState()) {
    init {
        launch {
            screenVisible.flatMapLatest { visible -> if (visible) feed.snapshots() else emptyFlow() }.collect(::apply)
        }
        launch { screenShown.collect { updateState { copy(fallbackReady = fallbackReady()) } } }
    }

    private fun apply(snapshot: VpnSnapshot) = updateState {
        copy(
            connection = snapshot.connection,
            traffic = snapshot.traffic,
            nowMillis = snapshot.nowMillis,
            profile = snapshot.profile,
            settings = snapshot.settings,
            atHome = snapshot.atHome,
        )
    }

    private fun fallbackReady(): Boolean = controller.availableTransports().any { it.external }

    override fun handle(intent: VpnIntent) {
        when (intent) {
            VpnIntent.ToggleConnection -> toggle()
            is VpnIntent.PermissionResult -> onPermission(intent.granted)
            is VpnIntent.ConnectVia -> commands.connectVia(intent.transport)
            is VpnIntent.SetRoute -> launch { settings.setRoute(intent.route).also { controller.reconnect() } }
            is VpnIntent.SetAuth -> launch { settings.setAuth(intent.auth) }
            is VpnIntent.SetAutoConnect -> launch { settings.setAutoConnect(intent.enabled) }
            is VpnIntent.SetConnectOnOpen -> launch { settings.setConnectOnOpen(intent.enabled) }
            VpnIntent.Reissue -> busy { actions.reissue() }
            VpnIntent.Forget -> busy { actions.forget() }
            is VpnIntent.ImportCertificate -> busy { actions.importCertificate(intent.uri, intent.password) }
            VpnIntent.InstallCertificate -> launch { actions.certificateInstall()?.let { emit(VpnEvent.Open(it)) } }
        }
    }

    private fun toggle() {
        val request = permission.request()
        when {
            currentState.connection.isActive -> commands.disconnect()
            request != null -> commands.wantUp().also { emit(VpnEvent.RequestPermission(request)) }
            else -> commands.connect()
        }
    }

    private fun onPermission(granted: Boolean) {
        when (granted) {
            true -> commands.connect()
            false -> emit(VpnEvent.Notice(VpnNotice.PERMISSION_DENIED))
        }
    }

    private fun busy(block: suspend () -> VpnNotice?) {
        launch { runBusy(block) }
    }

    private suspend fun runBusy(block: suspend () -> VpnNotice?) {
        updateState { copy(busy = true) }
        val notice = block()
        updateState { copy(busy = false) }
        notice?.let { emit(VpnEvent.Notice(it)) }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(VpnViewModel::class)
@Inject
class VpnViewModelSource internal constructor(
    private val viewModel: () -> VpnViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
