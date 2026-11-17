package dev.proflyder.home.di

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import dev.proflyder.home.core.network.NetworkWaiter
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Waits for a default network (Wi-Fi, LAN or the VPN tunnel) while the phone
 * wakes up after Doze. A resume can race the tunnel coming back, so the first
 * read should wait instead of immediately showing "check LAN or VPN".
 *
 * The call blocks on an OkHttp worker thread, never on the UI thread. When a
 * network already exists the check is a cheap field read.
 */
@SuppressLint("MissingPermission") // ACCESS_NETWORK_STATE is declared in the app manifest.
class AndroidNetworkWaiter(private val context: Context) : NetworkWaiter {

    override fun awaitAvailable(timeoutMillis: Long): Boolean {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return true
        if (manager.activeNetwork != null) return true
        val appeared = CountDownLatch(1)
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: android.net.Network) = appeared.countDown()
        }
        val registered = runCatching { manager.registerDefaultNetworkCallback(callback) }.isSuccess
        val available = registered && appeared.await(timeoutMillis, TimeUnit.MILLISECONDS)
        if (registered) runCatching { manager.unregisterNetworkCallback(callback) }
        return available || manager.activeNetwork != null
    }
}
