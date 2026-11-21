package dev.proflyder.home.feature.vpn.tunnel

import android.content.Context
import com.wireguard.android.backend.GoBackend
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.WireGuardConfigText
import dev.proflyder.home.feature.vpn.WireGuardStats
import dev.proflyder.home.feature.vpn.WireGuardUapi
import dev.proflyder.home.feature.vpn.data.VpnKeyVault
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@SingleIn(AppScope::class)
@Inject
class WireGuardTransport(
    private val context: Context,
    private val keys: VpnKeyVault,
) : VpnTransport {
    private val backend: GoBackend by lazy { GoBackend(context.applicationContext) }
    private val tunnel = HomeTunnel()

    @Volatile
    private var config: Config? = null

    override val protocol: VpnProtocol = VpnProtocol.WIREGUARD

    fun onAlwaysOn(action: () -> Unit) {
        GoBackend.setAlwaysOnCallback { action() }
    }

    override suspend fun up(profile: VpnProfile, route: VpnRoute): Boolean = withContext(Dispatchers.IO) {
        val privateKey = keys.keyPair().privateKey.toBase64()
        val text = WireGuardConfigText.render(profile.wireguard, privateKey, route)
        suspendRunCatching { start(Config.parse(text.byteInputStream())) }.isSuccess
    }

    private fun start(parsed: Config) {
        backend.setState(tunnel, Tunnel.State.UP, parsed)
        config = parsed
    }

    override suspend fun down() {
        withContext(Dispatchers.IO) { runCatching { backend.setState(tunnel, Tunnel.State.DOWN, null) } }
        config = null
    }

    override fun latestHandshakeMillis(): Long = stats().latestHandshakeMillis

    override fun traffic(): VpnTraffic = stats().let { VpnTraffic(rxBytes = it.rxBytes, txBytes = it.txBytes) }

    private fun stats(): WireGuardStats = when (config) {
        null -> WireGuardStats()
        else -> WireGuardUapi.parse(uapi())
    }

    private fun uapi(): String? = runCatching { configReader.invoke(null, handleField.getInt(backend)) as? String }
        .getOrNull()

    private val handleField by lazy { GoBackend::class.java.getDeclaredField(HANDLE_FIELD).apply { isAccessible = true } }

    private val configReader by lazy {
        GoBackend::class.java.getDeclaredMethod(CONFIG_METHOD, Int::class.javaPrimitiveType).apply { isAccessible = true }
    }

    val isUp: Boolean get() = config != null

    private class HomeTunnel : Tunnel {
        override fun getName(): String = NAME

        override fun onStateChange(newState: Tunnel.State) = Unit
    }

    private companion object {
        const val NAME = "home"
        const val HANDLE_FIELD = "currentTunnelHandle"
        const val CONFIG_METHOD = "wgGetConfig"
    }
}
