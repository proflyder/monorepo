package dev.proflyder.home.feature.vpn.transport.openconnect

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import dev.zacsweers.metro.Inject
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.distinctUntilChanged

internal fun interface SystemVpnNetwork {
    fun present(): Flow<Boolean>
}

@Inject
internal class AndroidSystemVpnNetwork(private val context: Context) : SystemVpnNetwork {
    @SuppressLint("MissingPermission")
    override fun present(): Flow<Boolean> = callbackFlow { watchVpnNetworks() }.distinctUntilChanged()

    @SuppressLint("MissingPermission")
    private suspend fun ProducerScope<Boolean>.watchVpnNetworks() {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val networks = ConcurrentHashMap.newKeySet<Network>()
        val callback = VpnNetworkCallback(networks) { trySend(networks.isNotEmpty()) }
        trySend(false)
        val registered = manager != null && runCatching { manager.registerNetworkCallback(VPN_REQUEST, callback) }.isSuccess
        awaitClose { if (registered) runCatching { manager.unregisterNetworkCallback(callback) } }
    }

    private class VpnNetworkCallback(
        private val networks: MutableSet<Network>,
        private val changed: () -> Unit,
    ) : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            networks.add(network)
            changed()
        }

        override fun onLost(network: Network) {
            networks.remove(network)
            changed()
        }
    }

    private companion object {
        val VPN_REQUEST: NetworkRequest = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_VPN)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            .build()
    }
}
