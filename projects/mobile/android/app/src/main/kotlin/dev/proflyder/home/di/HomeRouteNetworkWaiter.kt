package dev.proflyder.home.di

import dev.proflyder.home.core.network.NetworkWaiter

internal class HomeRouteNetworkWaiter(
    private val network: NetworkWaiter,
    private val awaitHomeRoute: () -> Unit,
) : NetworkWaiter {
    override fun awaitAvailable(timeoutMillis: Long): Boolean {
        awaitHomeRoute()
        return network.awaitAvailable(timeoutMillis)
    }
}
