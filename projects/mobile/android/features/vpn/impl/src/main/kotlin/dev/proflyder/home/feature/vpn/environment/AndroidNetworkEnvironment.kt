package dev.proflyder.home.feature.vpn.environment

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.info
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.net.InetAddress
import java.net.InetSocketAddress
import java.util.concurrent.ConcurrentHashMap
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn

@OptIn(ExperimentalCoroutinesApi::class)
@SingleIn(AppScope::class)
@Inject
internal class AndroidNetworkEnvironment(
    private val context: Context,
    connection: ConnectionSettingsStore,
    private val logger: AppLogger,
    scope: CoroutineScope,
    @IoDispatcher io: CoroutineDispatcher,
) : NetworkEnvironment {
    private val manager: ConnectivityManager? get() = context.getSystemService(ConnectivityManager::class.java)

    override val state: StateFlow<HomeNetworkState> = combine(
        physicalNetworks().settle(SETTLE),
        connection.config.map { it.endpoint }.distinctUntilChanged(),
        ::Probe,
    )
        .mapLatest(::classify)
        .flowOn(io)
        .distinctUntilChanged()
        .onEach { logger.info(TAG, "environment ${it.javaClass.simpleName}") }
        .stateIn(scope, SharingStarted.WhileSubscribed(STOP_TIMEOUT_MILLIS, 0), HomeNetworkState.Unknown)

    private var lastState: HomeNetworkState = HomeNetworkState.Unknown

    private class Probe(val networks: NetworkSnapshot, val endpoint: ServerEndpoint?)

    private class NetworkSnapshot(val networks: List<Network>, val blocked: Set<Network> = emptySet()) {
        val facts = NetworkFacts(networks.map { it.networkHandle }.toSet(), blocked.map { it.networkHandle }.toSet())
        val usable: List<Network> get() = networks - blocked
    }

    private fun classify(probe: Probe): HomeNetworkState {
        val endpoint = probe.endpoint
        val snapshot = probe.networks
        val state = EnvironmentClassifier.classify(snapshot.facts, endpoint != null) {
            endpoint != null && snapshot.usable.any { reachesHome(it, endpoint) }
        }
        return EnvironmentClassifier.keepKnownHome(state, lastState, snapshot.facts.handles).also { lastState = it }
    }

    @SuppressLint("MissingPermission")
    private fun physicalNetworks(): Flow<NetworkSnapshot> = callbackFlow { watchPhysicalNetworks() }

    @SuppressLint("MissingPermission")
    private suspend fun ProducerScope<NetworkSnapshot>.watchPhysicalNetworks() {
        val networks = ConcurrentHashMap.newKeySet<Network>()
        val blocked = ConcurrentHashMap.newKeySet<Network>()
        val callback = PhysicalNetworkCallback(networks, blocked) { trySend(NetworkSnapshot(networks.toList(), blocked.toSet())) }
        val registered = runCatching { manager?.registerNetworkCallback(PHYSICAL_REQUEST, callback) }.isSuccess
        if (!registered) trySend(NetworkSnapshot(fallbackNetworks()))
        awaitClose { if (registered) runCatching { manager?.unregisterNetworkCallback(callback) } }
    }

    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    private fun fallbackNetworks(): List<Network> {
        val manager = manager ?: return emptyList()
        return manager.allNetworks.filter { network -> manager.getNetworkCapabilities(network)?.let(::isPhysical) == true }
    }

    private fun isPhysical(capabilities: NetworkCapabilities): Boolean =
        capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)

    private fun reachesHome(network: Network, endpoint: ServerEndpoint): Boolean =
        resolve(network, endpoint.host).filter(HomeAddress::isPrivate).any { connects(network, it, endpoint.port) }

    private fun resolve(network: Network, host: String): List<InetAddress> =
        runCatching { network.getAllByName(host).toList() }.getOrDefault(emptyList())

    private fun connects(network: Network, address: InetAddress, port: Int): Boolean = runCatching {
        network.socketFactory.createSocket().use { it.connect(InetSocketAddress(address, port), PROBE_TIMEOUT_MILLIS) }
    }.isSuccess

    private class PhysicalNetworkCallback(
        private val networks: MutableSet<Network>,
        private val blocked: MutableSet<Network>,
        private val changed: () -> Unit,
    ) : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            networks.add(network)
            changed()
        }

        override fun onLost(network: Network) {
            networks.remove(network)
            blocked.remove(network)
            changed()
        }

        override fun onBlockedStatusChanged(network: Network, isBlocked: Boolean) {
            when (isBlocked) {
                true -> blocked.add(network)
                false -> blocked.remove(network)
            }
            changed()
        }

        override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) = changed()
    }

    private companion object {
        const val TAG = "VpnEnvironment"
        val SETTLE = 1_500.milliseconds
        val STOP_TIMEOUT_MILLIS = 5.seconds.inWholeMilliseconds
        const val PROBE_TIMEOUT_MILLIS = 1_500
        val PHYSICAL_REQUEST: NetworkRequest = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            .build()
    }
}
