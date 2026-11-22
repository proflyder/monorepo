package dev.proflyder.home.feature.vpn.tunnel

import android.content.Context
import android.content.Intent
import android.net.VpnService
import dev.zacsweers.metro.Inject

internal interface VpnPermission {
    fun request(): Intent?

    fun granted(): Boolean = request() == null
}

@Inject
internal class SystemVpnPermission(private val context: Context) : VpnPermission {
    override fun request(): Intent? = VpnService.prepare(context)
}
