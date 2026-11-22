package dev.proflyder.home.feature.vpn.tunnel

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.HomeRoute
import dev.proflyder.home.feature.vpn.NetworkState
import dev.proflyder.home.feature.vpn.NetworkVpnAction
import dev.proflyder.home.feature.vpn.NetworkVpnInput
import dev.proflyder.home.feature.vpn.OpenConnectRules
import dev.proflyder.home.feature.vpn.VpnNetworkFlow
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class VpnOpenConnector(
    private val controller: TunnelVpnController,
    private val settings: VpnSettingsStore,
    private val home: HomeNetworkMonitor,
    private val scope: CoroutineScope,
) : DefaultLifecycleObserver, AppStartable {
    private val mutableRoute = MutableStateFlow(HomeRoute.READY)
    private val permissionChannel = Channel<Unit>(Channel.CONFLATED)
    private var watcher: Job? = null
    private var observer: Job? = null
    private var reachability: Job? = null

    val route: StateFlow<HomeRoute> = mutableRoute.asStateFlow()

    val permissionRequests: Flow<Unit> = permissionChannel.receiveAsFlow()

    override fun start() {
        scope.launch(Dispatchers.Main) { ProcessLifecycleOwner.get().lifecycle.addObserver(this@VpnOpenConnector) }
    }

    override fun onStart(owner: LifecycleOwner) {
        mutableRoute.value = HomeRoute.CHECKING
        home.foreground.value = true
        observer?.cancel()
        observer = scope.launch { observe() }
    }

    override fun onStop(owner: LifecycleOwner) {
        observer?.cancel()
        reachability?.cancel()
        home.foreground.value = false
        release()
    }

    private suspend fun observe() {
        controller.awaitLoaded()
        reachability = scope.launch(start = CoroutineStart.UNDISPATCHED) { home.watch() }
        VpnNetworkFlow.decisions(home.state.filterNotNull(), ::input).collect(::apply)
    }

    private suspend fun apply(action: NetworkVpnAction) {
        when (action) {
            NetworkVpnAction.CONNECT -> connect()
            NetworkVpnAction.RECONNECT -> controller.restartIfActive().also { release() }
            NetworkVpnAction.DISCONNECT -> controller.disconnect().also { release() }
            NetworkVpnAction.ASK_PERMISSION -> askPermission()
            NetworkVpnAction.NONE -> release()
        }
    }

    private suspend fun askPermission() {
        settings.markPermissionAsked()
        permissionChannel.trySend(Unit)
    }

    fun connect() = follow { controller.connect() }

    fun release() {
        mutableRoute.value = HomeRoute.READY
    }

    private fun follow(start: () -> Unit) {
        mutableRoute.value = HomeRoute.CONNECTING
        start()
        watcher?.cancel()
        watcher = scope.launch { awaitSettled() }
    }

    private suspend fun awaitSettled() {
        withTimeoutOrNull(ROUTE_WAIT_MILLIS) { controller.connection.first(OpenConnectRules::settled) }
        release()
    }

    private fun input(network: NetworkState): NetworkVpnInput {
        val value = settings.settings.value
        return NetworkVpnInput(
            connectOnOpen = value.connectOnOpen,
            autoConnect = value.autoConnect,
            atHome = network.atHome,
            active = controller.connection.value.isActive,
            endpointStale = controller.endpointStale(),
            hasProfile = controller.hasProfile(),
            permissionGranted = controller.permissionIntent() == null,
            permissionAsked = value.permissionAsked,
        )
    }

    private companion object {
        const val ROUTE_WAIT_MILLIS = 20_000L
    }
}
