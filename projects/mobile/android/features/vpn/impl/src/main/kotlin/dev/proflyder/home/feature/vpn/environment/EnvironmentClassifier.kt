package dev.proflyder.home.feature.vpn.environment

import dev.proflyder.home.feature.vpn.HomeNetworkState

internal data class NetworkFacts(val handles: Set<Long>, val blocked: Set<Long> = emptySet()) {
    val usable: Set<Long> get() = handles - blocked
}

internal object EnvironmentClassifier {
    fun classify(facts: NetworkFacts, endpointKnown: Boolean, probe: () -> HomeProbe): HomeNetworkState = when {
        facts.handles.isEmpty() -> HomeNetworkState.Offline
        facts.usable.isEmpty() || !endpointKnown -> HomeNetworkState.Unknown
        else -> probed(probe(), facts.handles)
    }

    private fun probed(probe: HomeProbe, handles: Set<Long>): HomeNetworkState = when (probe) {
        HomeProbe.REACHED -> HomeNetworkState.Home(handles)
        HomeProbe.RESOLVED -> HomeNetworkState.Away(handles)
        HomeProbe.UNRESOLVED -> HomeNetworkState.Unknown
    }

    fun keepKnownHome(
        state: HomeNetworkState,
        previous: HomeNetworkState,
        handles: Set<Long>,
    ): HomeNetworkState = when {
        state == HomeNetworkState.Unknown &&
            previous is HomeNetworkState.Home &&
            previous.networks == handles -> previous
        else -> state
    }
}
