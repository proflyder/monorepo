package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import dev.proflyder.home.feature.vpn.VpnController
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

internal enum class TunnelWish {
    UP,
    DOWN,
}

internal data class UserVpnChoice(val wish: TunnelWish, val networks: Set<Long>?)

internal fun UserVpnChoice.on(environment: HomeNetworkState): UserVpnChoice? {
    val current = environment.physicalNetworks() ?: return this
    return when (networks) {
        null -> copy(networks = current)
        current -> this
        else -> null
    }
}

internal fun HomeNetworkState.physicalNetworks(): Set<Long>? = when (this) {
    is HomeNetworkState.Home -> networks
    is HomeNetworkState.Away -> networks
    HomeNetworkState.Offline -> emptySet()
    HomeNetworkState.Unknown -> null
}

@SingleIn(AppScope::class)
@Inject
internal class UserVpnIntent(private val environment: NetworkEnvironment) {
    private val mutableChoice = MutableStateFlow<UserVpnChoice?>(null)

    val choice: StateFlow<UserVpnChoice?> = mutableChoice.asStateFlow()

    fun record(wish: TunnelWish) {
        mutableChoice.value = UserVpnChoice(wish, environment.state.value.physicalNetworks())
    }

    fun wishOn(network: HomeNetworkState): TunnelWish? {
        mutableChoice.update { it?.on(network) }
        return mutableChoice.value?.wish
    }
}

@Inject
internal class UserVpnCommands(
    private val controller: VpnController,
    private val intent: UserVpnIntent,
) {
    fun connect() {
        intent.record(TunnelWish.UP)
        controller.connect()
    }

    fun connectVia(transport: VpnProtocolId) {
        intent.record(TunnelWish.UP)
        controller.connectVia(transport)
    }

    fun disconnect() {
        intent.record(TunnelWish.DOWN)
        controller.disconnect()
    }

    fun wantUp() = intent.record(TunnelWish.UP)
}
