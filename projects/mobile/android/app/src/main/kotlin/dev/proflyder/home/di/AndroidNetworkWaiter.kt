package dev.proflyder.home.di

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import dev.proflyder.home.core.network.NetworkWaiter
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@SuppressLint("MissingPermission")
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
