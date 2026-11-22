package dev.proflyder.home.core.runtime

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.stateIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidDefaultNetwork(
    context: Context,
    scope: CoroutineScope,
) : DefaultNetwork {
    private val manager: ConnectivityManager? = context.getSystemService(ConnectivityManager::class.java)

    override val handle: StateFlow<Long?> = (manager?.defaultNetworkChanges() ?: emptyFlow())
        .stateIn(scope, SharingStarted.WhileSubscribed(STOP_TIMEOUT_MILLIS), manager?.currentHandle())

    @SuppressLint("MissingPermission")
    private fun ConnectivityManager.currentHandle(): Long? = activeNetwork?.networkHandle

    @SuppressLint("MissingPermission")
    private fun ConnectivityManager.defaultNetworkChanges(): Flow<Long?> = callbackFlow {
        val callback = DefaultNetworkCallback { trySend(it) }
        val registered = runCatching { registerDefaultNetworkCallback(callback) }.isSuccess
        awaitClose { if (registered) runCatching { unregisterNetworkCallback(callback) } }
    }

    private companion object {
        const val STOP_TIMEOUT_MILLIS = 5_000L
    }
}

private class DefaultNetworkCallback(
    private val onHandle: (Long?) -> Unit,
) : ConnectivityManager.NetworkCallback() {
    override fun onAvailable(network: Network) = onHandle(network.networkHandle)

    override fun onLost(network: Network) = onHandle(null)
}
