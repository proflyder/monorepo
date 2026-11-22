package dev.proflyder.home.feature.vpn.transport.wireguard

import android.content.Context
import com.wireguard.android.backend.GoBackend
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.proflyder.home.feature.vpn.data.VpnKeyVault
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.transport.TransportStatus
import dev.proflyder.home.feature.vpn.transport.TunnelRequest
import dev.proflyder.home.feature.vpn.transport.VpnTransport
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.withContext

@OptIn(ExperimentalCoroutinesApi::class)
@SingleIn(AppScope::class)
@Inject
internal class WireGuardTransport(
    private val context: Context,
    private val keys: VpnKeyVault,
    @IoDispatcher private val io: CoroutineDispatcher,
) : VpnTransport {
    private val backend: GoBackend by lazy { GoBackend(context.applicationContext) }
    private val stats: WireGuardStatsSource = GoBackendUapi { backend }
    private val tunnel = HomeTunnel()
    private val running = MutableStateFlow(false)

    override val info = VpnTransportInfo(ID, R.string.vpn_protocol_wireguard, external = false)

    override val order: Int = 0

    override val needsSystemPermission: Boolean = true

    override val handshakeTimeout: Duration = HANDSHAKE_TIMEOUT

    override val status: Flow<TransportStatus> = running
        .flatMapLatest { up -> statusWhile(up) }
        .distinctUntilChanged()
        .flowOn(io)

    override val traffic: Flow<VpnTraffic?> = running
        .flatMapLatest { up -> trafficWhile(up) }
        .flowOn(io)

    override val systemStarts: Flow<Unit> = callbackFlow {
        GoBackend.setAlwaysOnCallback { trySend(Unit) }
        awaitClose { }
    }

    override fun needsRestart(current: TunnelRequest, next: TunnelRequest): Boolean {
        val settings = WireGuardSettings.of(current.settings) ?: return false
        return settings.endpoint(current.atHome) != settings.endpoint(next.atHome)
    }

    override suspend fun up(request: TunnelRequest): Boolean = withContext(io) { start(request) }

    private suspend fun start(request: TunnelRequest): Boolean {
        val settings = WireGuardSettings.of(request.settings) ?: return false
        val key = keys.keyPair().privateKey.toBase64()
        val text = WireGuardConfigText.render(settings, key, request.route, request.atHome)
        return suspendRunCatching { backend.setState(tunnel, Tunnel.State.UP, Config.parse(text.byteInputStream())) }
            .onSuccess { running.value = true }
            .isSuccess
    }

    override suspend fun down() {
        withContext(io) { runCatching { backend.setState(tunnel, Tunnel.State.DOWN, null) } }
        running.value = false
    }

    private fun statusWhile(up: Boolean): Flow<TransportStatus> = when (up) {
        true -> tickerFlow(HANDSHAKE_POLL).map { handshakeStatus() }.transformWhile { emit(it); it == TransportStatus.Pending }
        false -> flowOf(TransportStatus.Down)
    }

    private fun handshakeStatus(): TransportStatus = when (stats.read().latestHandshakeMillis > 0) {
        true -> TransportStatus.Up(verified = true)
        false -> TransportStatus.Pending
    }

    private fun trafficWhile(up: Boolean): Flow<VpnTraffic?> = when (up) {
        true -> tickerFlow(TRAFFIC_TICK).map { stats.read().traffic }
        false -> flowOf(null)
    }

    private class HomeTunnel : Tunnel {
        override fun getName(): String = NAME

        override fun onStateChange(newState: Tunnel.State) = Unit
    }

    companion object {
        val ID = VpnProtocolId("wireguard")
        private const val NAME = "home"
        private val HANDSHAKE_TIMEOUT = 10.seconds
        private val HANDSHAKE_POLL = 500.milliseconds
        private val TRAFFIC_TICK = 1.seconds
    }
}
