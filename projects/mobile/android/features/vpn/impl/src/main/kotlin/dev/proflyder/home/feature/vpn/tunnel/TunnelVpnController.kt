package dev.proflyder.home.feature.vpn.tunnel

import android.content.Context
import android.content.Intent
import android.net.VpnService
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.HandshakePolicy
import dev.proflyder.home.feature.vpn.HandshakeVerdict
import dev.proflyder.home.feature.vpn.HomeNetworkRules
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnEndpoints
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@ContributesBinding(AppScope::class, binding = binding<VpnController>())
@Inject
class TunnelVpnController(
    private val context: Context,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val wireGuard: WireGuardTransport,
    private val openConnect: ExternalOpenConnectTransport,
    private val scope: CoroutineScope,
) : VpnController, AppStartable {
    private val state = MutableStateFlow<VpnConnection>(VpnConnection.Disconnected)
    private var job: Job? = null
    private val loaded = CompletableDeferred<Unit>()

    @Volatile
    private var active: VpnTransport? = null

    override val connection: StateFlow<VpnConnection> = state.asStateFlow()

    override fun start() {
        wireGuard.onAlwaysOn { connect() }
        scope.launch { prepare() }
    }

    private suspend fun prepare() {
        settings.load()
        profiles.load()
        loaded.complete(Unit)
        profiles.ensure()
    }

    suspend fun awaitLoaded() = loaded.await()

    fun hasProfile(): Boolean = knownProfile() != null

    fun permissionIntent(): Intent? = VpnService.prepare(context)

    override fun connect() {
        if (job?.isActive == true) return
        state.value = VpnConnection.Connecting(VpnProtocol.WIREGUARD)
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

    fun homeChanged(atHome: Boolean) {
        wireGuard.atHome = atHome
    }

    fun followNetwork(autoConnect: Boolean) {
        val home = wireGuard.atHome == true
        val disconnecting = HomeNetworkRules.shouldDisconnect(autoConnect, home, wireGuardActive())
        if (endpointStale() && !disconnecting) restartIfActive()
    }

    fun endpointStale(): Boolean {
        val profile = knownProfile() ?: return false
        val wanted = VpnEndpoints.select(profile.wireguard, wireGuard.atHome)
        return wireGuardActive() && wireGuard.endpoint != wanted
    }

    private fun knownProfile(): VpnProfile? = when (val value = profiles.state.value) {
        is VpnProfileState.Ready -> value.profile
        is VpnProfileState.Failed -> value.profile
        else -> null
    }

    private fun wireGuardActive(): Boolean = when (val value = state.value) {
        is VpnConnection.Connecting -> value.protocol == VpnProtocol.WIREGUARD
        is VpnConnection.Connected -> value.protocol == VpnProtocol.WIREGUARD
        else -> false
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
        failed.down()
        state.value = when (failed.protocol) {
            VpnProtocol.WIREGUARD -> HandshakePolicy.afterWireGuardTimeout(wireGuard.atHome)
            VpnProtocol.OPENCONNECT -> VpnConnection.FallbackRequired
        }
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
