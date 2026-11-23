package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.info
import dev.proflyder.home.core.model.warn
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.proflyder.home.feature.vpn.atHome
import dev.proflyder.home.feature.vpn.transport.TransportStatus
import dev.proflyder.home.feature.vpn.transport.TunnelRequest
import dev.proflyder.home.feature.vpn.transport.VpnTransport
import dev.proflyder.home.feature.vpn.transport.VpnTransportChain
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.takeWhile
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

internal interface TunnelSwitch {
    val switching: StateFlow<Boolean>
}

internal sealed interface VpnCommand {
    data class Connect(val via: VpnProtocolId?, val origin: ConnectOrigin) : VpnCommand

    data class Reconnect(val origin: ConnectOrigin) : VpnCommand

    data object Disconnect : VpnCommand
}

@OptIn(ExperimentalCoroutinesApi::class)
@SingleIn(AppScope::class)
@Inject
internal class ChainVpnController(
    private val chain: VpnTransportChain,
    private val profiles: VpnProfiles,
    private val settings: VpnSettingsStore,
    private val environment: NetworkEnvironment,
    private val permission: VpnPermission,
    private val clock: Clock,
    private val logger: AppLogger,
    private val scope: CoroutineScope,
) : VpnController, TunnelSwitch, TunnelLauncher, AppStartable {
    private val state = MutableStateFlow<VpnConnection>(VpnConnection.Disconnected)
    private val switchingState = MutableStateFlow(false)
    private val commands = Channel<VpnCommand>(Channel.UNLIMITED)
    private var session: Job? = null
    private var active: ActiveTunnel? = null

    override val connection: StateFlow<VpnConnection> = state.asStateFlow()

    override val switching: StateFlow<Boolean> = switchingState.asStateFlow()

    override val traffic: Flow<VpnTraffic?> = connection
        .map { (it as? VpnConnection.Connected)?.transport?.id }
        .distinctUntilChanged()
        .flatMapLatest { id -> id?.let(chain::find)?.traffic ?: flowOf(null) }

    override fun start() {
        scope.launch { commands.receiveAsFlow().collect(::execute) }
    }

    override fun availableTransports(): List<VpnTransportInfo> = chain.available()

    override fun connect() = launch(ConnectOrigin.USER)

    override fun launch(origin: ConnectOrigin) = send(VpnCommand.Connect(via = null, origin))

    override fun connectVia(transport: VpnProtocolId) = send(VpnCommand.Connect(transport, ConnectOrigin.USER))

    override fun reconnect() = send(VpnCommand.Reconnect(ConnectOrigin.USER))

    override fun disconnect() = send(VpnCommand.Disconnect)

    private fun send(command: VpnCommand) {
        commands.trySend(command)
    }

    private suspend fun execute(command: VpnCommand) {
        switchingState.value = true
        perform(command)
        switchingState.value = false
    }

    private suspend fun perform(command: VpnCommand) {
        when (command) {
            is VpnCommand.Connect -> connect(command.via, command.origin)
            is VpnCommand.Reconnect -> restart(command.origin)
            VpnCommand.Disconnect -> stop()
        }
    }

    private suspend fun connect(via: VpnProtocolId?, origin: ConnectOrigin) {
        if (via == null && session?.isActive == true) return
        stop()
        session = scope.launch { run(via, origin) }
    }

    private suspend fun restart(origin: ConnectOrigin) {
        val current = active ?: return
        val via = current.transport.info.id.takeIf { current.transport.info.external }
        stop()
        session = scope.launch { run(via, origin) }
    }

    private suspend fun stop() {
        session?.cancelAndJoin()
        session = null
        active?.transport?.down()
        active = null
        state.value = VpnConnection.Disconnected
    }

    private suspend fun run(via: VpnProtocolId?, origin: ConnectOrigin) {
        val atHome = currentAtHome()
        logger.info(TAG, "connect origin=$origin atHome=$atHome via=${via?.value}")
        if (!ConnectGate.allows(origin, atHome)) return skip(origin, atHome)
        val profile = profiles.ensure() ?: return fail(VpnConnectProblem.NO_PROFILE)
        val candidates = via?.let { listOfNotNull(chain.find(it)) } ?: chain.automatic(profile)
        for (transport in candidates) {
            if (attempt(transport, profile, atHome) == Attempt.DONE) return
        }
        state.value = exhausted(profile, origin, environment.state.value.atHome ?: atHome)
    }

    private fun skip(origin: ConnectOrigin, atHome: Boolean?) {
        logger.warn(TAG, "connect skipped origin=$origin atHome=$atHome")
        state.value = VpnConnection.Disconnected
    }

    private suspend fun attempt(transport: VpnTransport, profile: VpnProfile, atHome: Boolean?): Attempt {
        state.value = VpnConnection.Connecting(transport.info)
        val request = request(transport, profile, atHome) ?: return Attempt.NEXT
        val problem = blocker(transport) ?: tunnelProblem(transport, request)
        if (problem != null) {
            fail(problem)
            return Attempt.DONE
        }
        active = ActiveTunnel(transport, request)
        val up = withTimeoutOrNull(transport.handshakeTimeout) { transport.status.first { it is TransportStatus.Up } }
        if (up == null) {
            giveUp(transport)
            return Attempt.NEXT
        }
        hold(transport, request)
        return Attempt.DONE
    }

    private suspend fun tunnelProblem(transport: VpnTransport, request: TunnelRequest): VpnConnectProblem? =
        VpnConnectProblem.TUNNEL.takeUnless { transport.up(request) }

    private fun blocker(transport: VpnTransport): VpnConnectProblem? = when {
        !transport.installed() -> VpnConnectProblem.NO_CLIENT
        transport.needsSystemPermission && !permission.granted() -> VpnConnectProblem.PERMISSION
        else -> null
    }

    private fun request(transport: VpnTransport, profile: VpnProfile, atHome: Boolean?): TunnelRequest? {
        val transportSettings = profile.settingsOf(transport.info.id) ?: return null
        val route = settings.settings.value?.route ?: VpnRoute.ALL
        return TunnelRequest(profile, transportSettings, route, atHome)
    }

    private suspend fun giveUp(transport: VpnTransport) {
        transport.down()
        active = null
    }

    private suspend fun hold(transport: VpnTransport, request: TunnelRequest) {
        coroutineScope { holdUntilDown(transport, request) }
        active = null
        state.value = VpnConnection.Disconnected
    }

    private suspend fun CoroutineScope.holdUntilDown(transport: VpnTransport, request: TunnelRequest) {
        val since = clock.wallMillis()
        val follower = launch { followEnvironment(transport, request) }
        transport.status.takeWhile { it != TransportStatus.Down }.collect { status ->
            if (status is TransportStatus.Up) state.value = VpnConnection.Connected(transport.info, since, status.verified)
        }
        follower.cancel()
    }

    private suspend fun followEnvironment(transport: VpnTransport, request: TunnelRequest) {
        environment.state.collect { network ->
            val next = request.copy(atHome = network.atHome ?: request.atHome)
            if (transport.needsRestart(request, next)) send(VpnCommand.Reconnect(ConnectOrigin.NETWORK_RESTART))
        }
    }

    private suspend fun currentAtHome(): Boolean? =
        withTimeoutOrNull(ENVIRONMENT_WAIT) { environment.state.first { it != HomeNetworkState.Unknown } }?.atHome

    private fun exhausted(profile: VpnProfile, origin: ConnectOrigin, atHome: Boolean?): VpnConnection {
        val outcome = ConnectGate.outcome(origin, atHome)
        logger.warn(TAG, "no handshake origin=$origin atHome=$atHome outcome=$outcome")
        return when (outcome) {
            ExhaustedOutcome.NOT_NEEDED_AT_HOME -> VpnConnection.NotNeededAtHome
            ExhaustedOutcome.OFFER_FALLBACK -> VpnConnection.FallbackRequired(chain.fallbacks(profile))
            ExhaustedOutcome.QUIET -> VpnConnection.Disconnected
        }
    }

    private fun fail(problem: VpnConnectProblem) {
        state.value = VpnConnection.Failed(problem)
    }

    private enum class Attempt {
        NEXT,
        DONE,
    }

    private class ActiveTunnel(val transport: VpnTransport, val request: TunnelRequest)

    private companion object {
        const val TAG = "VpnController"
        val ENVIRONMENT_WAIT = 10.seconds
    }
}
