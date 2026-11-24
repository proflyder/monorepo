package dev.proflyder.home.feature.vpn

import android.content.Intent
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.ForegroundState
import dev.proflyder.home.feature.vpn.transport.SystemVpnStarts
import dev.proflyder.home.feature.vpn.transport.TransportStatus
import dev.proflyder.home.feature.vpn.transport.TunnelRequest
import dev.proflyder.home.feature.vpn.transport.VpnTransport
import dev.proflyder.home.feature.vpn.tunnel.ConnectOrigin
import dev.proflyder.home.feature.vpn.tunnel.TunnelLauncher
import dev.proflyder.home.feature.vpn.tunnel.VpnPermission
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update

internal class FakeSettings(initial: VpnSettings? = VpnSettings()) : VpnSettingsStore {
    override val settings = MutableStateFlow(initial)

    override suspend fun setRoute(route: VpnRoute) = change { copy(route = route) }

    override suspend fun setAutoConnect(enabled: Boolean) = change { copy(autoConnect = enabled) }

    override suspend fun setConnectOnOpen(enabled: Boolean) = change { copy(connectOnOpen = enabled) }

    override suspend fun markPermissionAsked() = change { copy(permissionAsked = true) }

    override suspend fun setAuth(auth: VpnAuth) = change { copy(auth = auth) }

    override suspend fun setCertificateName(name: String) = change { copy(certificateName = name) }

    private fun change(block: VpnSettings.() -> VpnSettings) = settings.update { (it ?: VpnSettings()).block() }
}

internal class FakeProfiles(profile: VpnProfile? = VpnSamples.profile) : VpnProfiles {
    override val state = MutableStateFlow(profile?.let(VpnProfileState::Ready) ?: VpnProfileState.Missing)

    override suspend fun ensure(): VpnProfile? = state.value.knownProfile

    override suspend fun reissue(): Boolean = true

    override suspend fun forget(): Boolean = true
}

internal class FakeEnvironment(initial: HomeNetworkState = HomeNetworkState.Unknown) : NetworkEnvironment {
    override val state = MutableStateFlow(initial)
}

internal class FakeForeground(foreground: Boolean) : AppForeground {
    override val state = MutableStateFlow(ForegroundState(foreground, 0))

    fun move(foreground: Boolean) {
        state.value = ForegroundState(foreground, 0)
    }
}

internal class FakeActivation(active: Boolean = true) : AppActivation {
    override val active = MutableStateFlow(active)

    override fun activate() {
        this.active.value = true
    }

    override suspend fun awaitActive() {
        this.active.first { it }
    }
}

internal class FakePermission(var allowed: Boolean = true) : VpnPermission {
    override fun request(): Intent? = null

    override fun granted(): Boolean = allowed
}

internal object FakeClock : Clock {
    override fun wallMillis(): Long = 1_000

    override fun elapsedNanos(): Long = 0
}

internal class FakeSystemStarts : SystemVpnStarts {
    override val starts = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
}

internal class FakeController : VpnController, TunnelLauncher {
    val commands = mutableListOf<String>()
    val origins = mutableListOf<ConnectOrigin>()

    override val connection = MutableStateFlow<VpnConnection>(VpnConnection.Disconnected)

    override val traffic: Flow<VpnTraffic?> = flowOf(null)

    override fun availableTransports(): List<VpnTransportInfo> = emptyList()

    override fun connect() = launch(ConnectOrigin.USER)

    override fun launch(origin: ConnectOrigin) {
        commands += "connect"
        origins += origin
        connection.value = VpnConnection.Connecting(FakeTransport.info("primary"))
    }

    override fun connectVia(transport: VpnProtocolId) {
        commands += "connect:${transport.value}"
    }

    override fun reconnect() {
        commands += "reconnect"
    }

    override fun disconnect() {
        commands += "disconnect"
        connection.value = VpnConnection.Disconnected
    }
}

internal enum class Handshake {
    ANSWERS,
    SILENT,
}

internal class FakeTransport(
    id: String,
    override val order: Int,
    var handshake: Handshake,
    external: Boolean = false,
    override val needsSystemPermission: Boolean = !external,
    private val restartOnNetworkChange: Boolean = false,
) : VpnTransport {
    private val current = MutableStateFlow<TransportStatus>(TransportStatus.Down)
    val requests = mutableListOf<TunnelRequest>()
    var downs = 0

    override val info = info(id, external)

    override val handshakeTimeout: Duration = 10.seconds

    override val status: StateFlow<TransportStatus> = current

    override fun needsRestart(current: TunnelRequest, next: TunnelRequest): Boolean =
        restartOnNetworkChange && current.atHome != next.atHome

    override suspend fun up(request: TunnelRequest): Boolean {
        requests += request
        current.value = when (handshake) {
            Handshake.ANSWERS -> TransportStatus.Up(verified = !info.external)
            Handshake.SILENT -> TransportStatus.Pending
        }
        return true
    }

    override suspend fun down() {
        downs += 1
        current.value = TransportStatus.Down
    }

    companion object {
        fun info(id: String, external: Boolean = false) = VpnTransportInfo(VpnProtocolId(id), 0, external)
    }
}
