package dev.proflyder.home.feature.vpn.tunnel

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.feature.vpn.HomeNetworkRules
import dev.proflyder.home.feature.vpn.HomeReachability
import dev.proflyder.home.feature.vpn.VpnNetworkFlow
import android.net.LinkProperties
import dev.proflyder.home.feature.vpn.NetworkState
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.net.InetSocketAddress
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@SingleIn(AppScope::class)
@Inject
class HomeNetworkMonitor(
    private val context: Context,
    private val connection: ConnectionSettingsStore,
    private val settings: VpnSettingsStore,
    private val controller: TunnelVpnController,
    private val scope: CoroutineScope,
) : HomeReachability {
    private val networks = ConcurrentHashMap.newKeySet<Network>()
    private val home = MutableStateFlow<Boolean?>(null)
    private var pending: Job? = null

    val atHome: StateFlow<Boolean?> = home.asStateFlow()

    val foreground = MutableStateFlow(false)

    private val reachable = MutableStateFlow<NetworkState?>(null)

    override val state: StateFlow<NetworkState?> = reachable.asStateFlow()

    override suspend fun watch() {
        reachable.value = null
        VpnNetworkFlow.settle(defaultNetworkChanges()).collect { reachable.value = snapshot() }
    }

    @SuppressLint("MissingPermission")
    fun start() {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return
        val request = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            .build()
        runCatching { manager.registerNetworkCallback(request, callback) }
    }

    fun refresh() = schedule()

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            networks.add(network)
            schedule()
        }

        override fun onLost(network: Network) {
            networks.remove(network)
            schedule()
        }
    }

    internal fun schedule() {
        pending?.cancel()
        pending = scope.launch { evaluate() }
    }

    private fun defaultNetworkChanges(): Flow<Unit> = callbackFlow {
        val listener = DefaultNetworkListener { trySend(Unit) }
        val registered = register(listener)
        awaitClose { unregister(listener, registered) }
    }

    @SuppressLint("MissingPermission")
    private fun register(listener: DefaultNetworkListener): Boolean {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val registered = manager != null && runCatching { manager.registerDefaultNetworkCallback(listener) }.isSuccess
        if (!registered) listener.changed()
        return registered
    }

    private suspend fun snapshot(): NetworkState {
        val atHomeNow = checkNow()
        val ids = networks.toList().ifEmpty { physicalNetworks() }.map { it.networkHandle }.toSet()
        return NetworkState(atHomeNow, ids)
    }

    private fun unregister(listener: DefaultNetworkListener, registered: Boolean) {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        if (registered) runCatching { manager?.unregisterNetworkCallback(listener) }
    }

    private class DefaultNetworkListener(val changed: () -> Unit) : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = changed()

        override fun onLost(network: Network) = changed()

        override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) = changed()
    }

    suspend fun checkNow(): Boolean? {
        val candidates = networks.toList().ifEmpty { physicalNetworks() }
        if (candidates.isEmpty()) return null
        val atHomeNow = candidates.any { probe(it) }
        home.value = atHomeNow
        controller.homeChanged(atHomeNow)
        return atHomeNow
    }

    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    private fun physicalNetworks(): List<Network> {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return emptyList()
        return manager.allNetworks.filter { isPhysical(manager, it) }
    }

    @SuppressLint("MissingPermission")
    private fun isPhysical(manager: ConnectivityManager, network: Network): Boolean {
        val capabilities = manager.getNetworkCapabilities(network) ?: return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
    }

    private suspend fun evaluate() {
        delay(SETTLE_MILLIS)
        val atHomeNow = checkNow() ?: false
        if (foreground.value) return
        val auto = settings.settings.value.autoConnect
        val active = controller.connection.value.isActive
        when {
            HomeNetworkRules.shouldConnect(auto, atHomeNow, active) -> controller.connect()
            HomeNetworkRules.shouldDisconnect(auto, atHomeNow, active) -> controller.disconnect()
            else -> controller.followNetwork(auto)
        }
    }

    private suspend fun probe(network: Network): Boolean = withContext(Dispatchers.IO) {
        val endpoint = connection.config.value.endpoint ?: return@withContext false
        val addresses = runCatching { network.getAllByName(endpoint.host).toList() }.getOrDefault(emptyList())
        addresses.filter(HomeNetworkRules::isHomeAddress).any { reachable(network, it, endpoint.port) }
    }

    private fun reachable(network: Network, address: java.net.InetAddress, port: Int): Boolean = runCatching {
        network.socketFactory.createSocket().use { it.connect(InetSocketAddress(address, port), PROBE_TIMEOUT_MILLIS) }
    }.isSuccess

    private companion object {
        const val SETTLE_MILLIS = 1_500L
        const val PROBE_TIMEOUT_MILLIS = 1_500
    }
}
