package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.NetworkEnvironment
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnSettings
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.isActive
import dev.proflyder.home.feature.vpn.knownProfile
import dev.proflyder.home.feature.vpn.transport.SystemVpnStarts
import dev.proflyder.home.feature.vpn.tunnel.ConnectOrigin
import dev.proflyder.home.feature.vpn.tunnel.TunnelLauncher
import dev.proflyder.home.feature.vpn.tunnel.VpnPermission
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

@OptIn(ExperimentalCoroutinesApi::class)
@SingleIn(AppScope::class)
@Inject
internal class AutoConnectCoordinator(
    private val controller: VpnController,
    private val launcher: TunnelLauncher,
    private val permission: VpnPermission,
    private val settings: VpnSettingsStore,
    private val profiles: VpnProfiles,
    private val environment: NetworkEnvironment,
    private val foreground: AppForeground,
    private val userIntent: UserVpnIntent,
    private val systemStarts: SystemVpnStarts,
    private val journal: AutoConnectJournal,
    private val activation: AppActivation,
    private val scope: CoroutineScope,
) : AppStartable {
    private val mutableRoute = MutableStateFlow(HomeRoute.READY)
    private val permissionChannel = Channel<Unit>(Channel.CONFLATED)
    private var manual: Job? = null

    val route: StateFlow<HomeRoute> = mutableRoute.asStateFlow()

    val permissionRequests: Flow<Unit> = permissionChannel.receiveAsFlow()

    override fun start() {
        scope.launch {
            activation.awaitActive()
            triggers().collectLatest(::decide)
        }
        scope.launch { systemStarts.starts.collect { onSystemStart() } }
    }

    fun connectNow() {
        manual?.cancel()
        manual = scope.launch { connectAndWait(ConnectOrigin.USER) }
    }

    fun permissionResult(granted: Boolean) {
        when (granted) {
            true -> connectNow()
            false -> mutableRoute.value = HomeRoute.READY
        }
    }

    private fun triggers(): Flow<AutoConnectTrigger> = combine(
        foreground.state.map { it.foreground }.distinctUntilChanged(),
        settings.settings.filterNotNull().map { it.autoConnect }.distinctUntilChanged(),
        controller.connection.map { it.isActive }.distinctUntilChanged(),
        ::Watch,
    )
        .flatMapLatest(::withEnvironment)
        .distinctUntilChanged()

    private fun withEnvironment(watch: Watch): Flow<AutoConnectTrigger> = when (watch.needsEnvironment) {
        true -> environment.readings(ENVIRONMENT_TIMEOUT).map { watch.trigger(it) }
        false -> flowOf(watch.trigger(EnvironmentReading(HomeNetworkState.Unknown)))
    }

    private suspend fun decide(trigger: AutoConnectTrigger) {
        val input = input(trigger.foreground, trigger.environment)
        val decision = AutoConnectPolicy.decide(input)
        journal.decided(trigger.source, decision, input)
        when (decision.action) {
            AutoConnectAction.CONNECT -> connectAndWait(ConnectOrigin.AUTO_POLICY)
            AutoConnectAction.ASK_PERMISSION -> askPermission()
            AutoConnectAction.DISCONNECT -> disconnect()
            AutoConnectAction.NONE -> mutableRoute.value = idleRoute(trigger)
        }
    }

    private suspend fun onSystemStart() {
        val input = input(foreground.state.value.foreground, environment.settledState(ENVIRONMENT_TIMEOUT))
        val decision = AutoConnectPolicy.decide(input)
        journal.decided(DecisionSource.SYSTEM_START, decision, input)
        if (decision.action == AutoConnectAction.CONNECT) launcher.launch(ConnectOrigin.AUTO_POLICY)
    }

    private fun idleRoute(trigger: AutoConnectTrigger): HomeRoute = when {
        trigger.foreground && trigger.environment == HomeNetworkState.Unknown && !trigger.undetermined -> HomeRoute.CHECKING
        else -> HomeRoute.READY
    }

    private suspend fun connectAndWait(origin: ConnectOrigin) {
        mutableRoute.value = HomeRoute.CONNECTING
        launcher.launch(origin)
        withTimeoutOrNull(ROUTE_WAIT) { controller.connection.dropWhile { it !is VpnConnection.Connecting }.first(::settled) }
        mutableRoute.value = HomeRoute.READY
    }

    private suspend fun askPermission() {
        settings.markPermissionAsked()
        permissionChannel.trySend(Unit)
    }

    private fun disconnect() {
        controller.disconnect()
        mutableRoute.value = HomeRoute.READY
    }

    private fun settled(connection: VpnConnection): Boolean = connection !is VpnConnection.Connecting

    private fun input(foreground: Boolean, environment: HomeNetworkState): AutoConnectInput {
        val current = settings.settings.value ?: VpnSettings()
        return AutoConnectInput(
            foreground = foreground,
            autoConnect = current.autoConnect,
            connectOnOpen = current.connectOnOpen,
            environment = environment,
            active = controller.connection.value.isActive,
            hasProfile = profiles.state.value.knownProfile != null,
            permissionGranted = permission.granted(),
            permissionAsked = current.permissionAsked,
            userWish = userIntent.wishOn(environment),
        )
    }

    private data class Watch(val foreground: Boolean, val autoConnect: Boolean, val active: Boolean) {
        val needsEnvironment: Boolean get() = foreground || autoConnect || active

        fun trigger(reading: EnvironmentReading) =
            AutoConnectTrigger(foreground, autoConnect, reading.state, reading.undetermined)
    }

    private companion object {
        val ROUTE_WAIT = 20.seconds
        val ENVIRONMENT_TIMEOUT = 15.seconds
    }
}

internal data class AutoConnectTrigger(
    val foreground: Boolean,
    val autoConnect: Boolean,
    val environment: HomeNetworkState,
    val undetermined: Boolean = false,
) {
    val source: DecisionSource
        get() = when (undetermined) {
            true -> DecisionSource.ENVIRONMENT_TIMEOUT
            false -> DecisionSource.NETWORK_OR_SETTINGS
        }
}
