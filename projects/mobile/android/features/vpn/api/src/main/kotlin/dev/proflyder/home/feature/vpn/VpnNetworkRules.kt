package dev.proflyder.home.feature.vpn

import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.withIndex

data class NetworkState(val atHome: Boolean?, val networks: Set<Long> = emptySet())

interface HomeReachability {
    val state: StateFlow<NetworkState?>

    suspend fun watch()
}

enum class NetworkVpnAction {
    NONE,
    CONNECT,
    ASK_PERMISSION,
    RECONNECT,
    DISCONNECT,
}

data class NetworkVpnInput(
    val connectOnOpen: Boolean,
    val autoConnect: Boolean,
    val atHome: Boolean?,
    val active: Boolean,
    val endpointStale: Boolean,
    val hasProfile: Boolean,
    val permissionGranted: Boolean,
    val permissionAsked: Boolean,
)

object NetworkVpnRules {
    fun decide(input: NetworkVpnInput): NetworkVpnAction = when {
        input.atHome == null -> NetworkVpnAction.NONE
        input.atHome && input.autoConnect && input.active -> NetworkVpnAction.DISCONNECT
        input.endpointStale -> NetworkVpnAction.RECONNECT
        !wanted(input) -> NetworkVpnAction.NONE
        input.permissionGranted -> NetworkVpnAction.CONNECT
        input.permissionAsked -> NetworkVpnAction.NONE
        else -> NetworkVpnAction.ASK_PERMISSION
    }

    private fun wanted(input: NetworkVpnInput): Boolean =
        input.atHome == false && !input.active && input.hasProfile && (input.connectOnOpen || input.autoConnect)
}

object VpnNetworkFlow {
    const val DEBOUNCE_MILLIS: Long = 1_500

    @OptIn(FlowPreview::class)
    fun <T> settle(changes: Flow<T>, windowMillis: Long = DEBOUNCE_MILLIS): Flow<T> = changes
        .withIndex()
        .debounce { firstImmediately(it.index, windowMillis) }
        .map { it.value }

    fun decisions(states: Flow<NetworkState>, input: (NetworkState) -> NetworkVpnInput): Flow<NetworkVpnAction> =
        states.distinctUntilChanged().map { NetworkVpnRules.decide(input(it)) }

    private fun firstImmediately(index: Int, windowMillis: Long): Long = when (index) {
        0 -> 0
        else -> windowMillis
    }
}
