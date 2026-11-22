package dev.proflyder.home.feature.vpn

import kotlinx.coroutines.flow.StateFlow

sealed interface HomeNetworkState {
    data object Unknown : HomeNetworkState

    data object Offline : HomeNetworkState

    data class Home(val networks: Set<Long>) : HomeNetworkState

    data class Away(val networks: Set<Long>) : HomeNetworkState
}

val HomeNetworkState.atHome: Boolean?
    get() = when (this) {
        is HomeNetworkState.Home -> true
        is HomeNetworkState.Away -> false
        else -> null
    }

interface NetworkEnvironment {
    val state: StateFlow<HomeNetworkState>
}
