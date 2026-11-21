package dev.proflyder.home.feature.vpn.tunnel

import android.content.Context
import android.content.Intent
import android.net.VpnService
import dev.proflyder.home.feature.vpn.HandshakePolicy
import dev.proflyder.home.feature.vpn.HandshakeVerdict
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class TunnelVpnController(
    private val context: Context,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val wireGuard: WireGuardTransport,
    private val openConnect: ExternalOpenConnectTransport,
    private val scope: CoroutineScope,
) : VpnController {
    private val state = MutableStateFlow<VpnConnection>(VpnConnection.Disconnected)
    private var job: Job? = null

    @Volatile
    private var active: VpnTransport? = null

    override val connection: StateFlow<VpnConnection> = state.asStateFlow()

    fun start() {
        wireGuard.onAlwaysOn { connect() }
        scope.launch { prepare() }
    }

    private suspend fun prepare() {
        settings.load()
        profiles.load()
        profiles.ensure()
    }

    fun permissionIntent(): Intent? = VpnService.prepare(context)

    override fun connect() {
        if (job?.isActive == true) return
        VpnStatusService.start(context)
        job = scope.launch { runWireGuard() }
    }

    fun connectExternal() {
        job?.cancel()
        VpnStatusService.start(context)
        job = scope.launch { runExternal() }
    }

    override fun disconnect() {
        job?.cancel()
        scope.launch { stop() }
    }

    fun restartIfActive() {
        if (!state.value.isActive) return
        job?.cancel()
        job = scope.launch { restart() }
    }

    private suspend fun restart() {
        stop()
        VpnStatusService.start(context)
        runWireGuard()
    }

    private suspend fun runWireGuard() {
        state.value = VpnConnection.Connecting(VpnProtocol.WIREGUARD)
        val profile = profiles.ensure() ?: return fail(VpnConnectProblem.NO_PROFILE)
        when {
            permissionIntent() != null -> fail(VpnConnectProblem.PERMISSION)
            !wireGuard.up(profile, settings.settings.value.route) -> fail(VpnConnectProblem.TUNNEL)
            else -> follow(wireGuard)
        }
    }

    private suspend fun runExternal() {
        state.value = VpnConnection.Connecting(VpnProtocol.OPENCONNECT)
        val profile = profiles.ensure() ?: return fail(VpnConnectProblem.NO_PROFILE)
        when {
            !openConnect.clientInstalled() -> fail(VpnConnectProblem.NO_EXTERNAL_CLIENT)
            !openConnect.up(profile, settings.settings.value.route) -> state.value = VpnConnection.FallbackRequired
            else -> follow(openConnect)
        }
    }

    private suspend fun follow(transport: VpnTransport) {
        active = transport
        val started = System.currentTimeMillis()
        val timeout = HandshakePolicy.timeoutFor(transport.protocol)
        var verdict = HandshakeVerdict.WAIT
        while (verdict == HandshakeVerdict.WAIT) {
            delay(POLL_MILLIS)
            verdict = HandshakePolicy.verdict(started, now(), transport.latestHandshakeMillis(), timeout)
        }
        when (verdict) {
            HandshakeVerdict.CONNECTED -> monitor(transport)
            else -> fallback(transport)
        }
    }

    private suspend fun monitor(transport: VpnTransport) {
        val since = now()
        while (alive(transport)) {
            val traffic = transport.traffic()
            state.value = VpnConnection.Connected(transport.protocol, since, traffic.rxBytes, traffic.txBytes)
            delay(TICK_MILLIS)
        }
        active = null
        state.value = VpnConnection.Disconnected
    }

    private fun alive(transport: VpnTransport): Boolean = when (transport.protocol) {
        VpnProtocol.WIREGUARD -> wireGuard.isUp
        VpnProtocol.OPENCONNECT -> transport.latestHandshakeMillis() > 0
    }

    private suspend fun fallback(failed: VpnTransport) {
        active = null
        when (failed.protocol) {
            VpnProtocol.WIREGUARD -> switchToExternal(failed)
            VpnProtocol.OPENCONNECT -> state.value = VpnConnection.FallbackRequired
        }
    }

    private suspend fun switchToExternal(failed: VpnTransport) {
        failed.down()
        runExternal()
    }

    private suspend fun stop() {
        active?.takeIf { it.protocol == VpnProtocol.OPENCONNECT }?.down()
        wireGuard.down()
        active = null
        state.value = VpnConnection.Disconnected
    }

    private fun fail(problem: VpnConnectProblem) {
        state.value = VpnConnection.Failed(problem)
    }

    private fun now(): Long = System.currentTimeMillis()

    private companion object {
        const val POLL_MILLIS = 500L
        const val TICK_MILLIS = 1_000L
    }
}
