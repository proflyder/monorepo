package dev.proflyder.home.feature.vpn.tunnel

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import androidx.core.net.toUri
import dev.proflyder.home.core.model.VpnOcservSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.feature.vpn.OpenConnectLink
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@Inject
class ExternalOpenConnectTransport(
    private val context: Context,
) : VpnTransport {
    override val protocol: VpnProtocol = VpnProtocol.OPENCONNECT

    @Volatile
    private var launched = false

    override suspend fun up(profile: VpnProfile, route: VpnRoute): Boolean {
        val intent = connectIntent(profile.ocserv, route) ?: return false
        launched = launch(context, intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return launched
    }

    override suspend fun down() {
        if (launched) launch(context, view(DISCONNECT_URI).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        launched = false
    }

    override fun latestHandshakeMillis(): Long = when (launched && systemVpnActive()) {
        true -> System.currentTimeMillis()
        false -> 0
    }

    override fun traffic(): VpnTraffic = VpnTraffic()

    fun connectIntent(settings: VpnOcservSettings, route: VpnRoute): Intent? {
        val name = context.getString(R.string.vpn_external_profile_name)
        val link = OpenConnectLink.connect(settings, route, name) ?: return null
        return view(link).takeIf { resolvable(it) } ?: openConnectApp()
    }

    fun clientInstalled(): Boolean = resolvable(view("${OpenConnectLink.SCHEME}://")) || openConnectApp() != null

    private fun openConnectApp(): Intent? = context.packageManager.getLaunchIntentForPackage(OPENCONNECT_PACKAGE)

    @SuppressLint("QueryPermissionsNeeded")
    private fun resolvable(intent: Intent): Boolean = intent.resolveActivity(context.packageManager) != null

    @SuppressLint("MissingPermission")
    private fun systemVpnActive(): Boolean {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return false
        val capabilities = manager.getNetworkCapabilities(manager.activeNetwork) ?: return false
        return capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
    }

    private fun view(uri: String): Intent = Intent(Intent.ACTION_VIEW, uri.toUri())

    private fun launch(from: Context, intent: Intent): Boolean =
        runCatching { from.startActivity(intent) }
            .onFailure { failure -> if (failure !is ActivityNotFoundException && failure !is SecurityException) throw failure }
            .isSuccess

    private companion object {
        const val OPENCONNECT_PACKAGE = "app.openconnect"
        const val DISCONNECT_URI = "anyconnect://disconnect"
    }
}
