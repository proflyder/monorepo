package dev.proflyder.home.feature.vpn.environment

import java.net.InetAddress

internal enum class HomeProbe {
    REACHED,
    RESOLVED,
    UNRESOLVED,
}

internal fun homeProbe(addresses: List<InetAddress>, connects: (InetAddress) -> Boolean): HomeProbe = when {
    addresses.isEmpty() -> HomeProbe.UNRESOLVED
    addresses.any { HomeAddress.isPrivate(it) && connects(it) } -> HomeProbe.REACHED
    else -> HomeProbe.RESOLVED
}

internal fun List<HomeProbe>.best(): HomeProbe = when {
    contains(HomeProbe.REACHED) -> HomeProbe.REACHED
    contains(HomeProbe.RESOLVED) -> HomeProbe.RESOLVED
    else -> HomeProbe.UNRESOLVED
}
