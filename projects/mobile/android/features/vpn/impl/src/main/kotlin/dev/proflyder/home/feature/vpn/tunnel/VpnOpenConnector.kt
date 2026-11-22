package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.feature.vpn.HomeRoute
import dev.proflyder.home.feature.vpn.OpenConnectAction
import dev.proflyder.home.feature.vpn.OpenConnectInput
import dev.proflyder.home.feature.vpn.OpenConnectRules
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull

@SingleIn(AppScope::class)
@Inject
class VpnOpenConnector(
    private val controller: TunnelVpnController,
    private val settings: VpnSettingsStore,
    private val home: HomeNetworkMonitor,
    private val scope: CoroutineScope,
) {
    private val mutableRoute = MutableStateFlow(HomeRoute.READY)
    private var watcher: Job? = null

    val route: StateFlow<HomeRoute> = mutableRoute.asStateFlow()

    suspend fun onForeground(): OpenConnectAction {
        mutableRoute.value = HomeRoute.CHECKING
        controller.awaitLoaded()
        val action = OpenConnectRules.decide(input(home.checkNow()))
        when (action) {
            OpenConnectAction.CONNECT -> connect()
            OpenConnectAction.ASK_PERMISSION -> settings.markPermissionAsked()
            OpenConnectAction.NONE -> release()
        }
        return action
    }

    fun connect() {
        mutableRoute.value = HomeRoute.CONNECTING
        controller.connect()
        watcher?.cancel()
        watcher = scope.launch { awaitSettled() }
    }

    fun release() {
        mutableRoute.value = HomeRoute.READY
    }

    fun awaitReady(timeoutMillis: Long = ROUTE_WAIT_MILLIS) {
        if (mutableRoute.value == HomeRoute.READY) return
        runBlocking { withTimeoutOrNull(timeoutMillis) { route.first { it == HomeRoute.READY } } }
    }

    private suspend fun awaitSettled() {
        withTimeoutOrNull(ROUTE_WAIT_MILLIS) { controller.connection.first(OpenConnectRules::settled) }
        release()
    }

    private fun input(atHome: Boolean?): OpenConnectInput {
        val value = settings.settings.value
        return OpenConnectInput(
            enabled = value.connectOnOpen,
            atHome = atHome,
            active = controller.connection.value.isActive,
            hasProfile = controller.hasProfile(),
            permissionGranted = controller.permissionIntent() == null,
            permissionAsked = value.permissionAsked,
        )
    }

    private companion object {
        const val ROUTE_WAIT_MILLIS = 20_000L
    }
}
