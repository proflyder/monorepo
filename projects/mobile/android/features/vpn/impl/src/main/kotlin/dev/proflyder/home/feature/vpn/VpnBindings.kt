package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.network.RouteCondition
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.vpn.connect.AutoConnectCoordinator
import dev.proflyder.home.feature.vpn.connect.VpnRouteCondition
import dev.proflyder.home.feature.vpn.data.DataStoreVpnSettingsStore
import dev.proflyder.home.feature.vpn.data.RemoteVpnProfiles
import dev.proflyder.home.feature.vpn.data.VpnProfileSync
import dev.proflyder.home.feature.vpn.environment.AndroidNetworkEnvironment
import dev.proflyder.home.feature.vpn.transport.SystemVpnStarts
import dev.proflyder.home.feature.vpn.transport.VpnTransport
import dev.proflyder.home.feature.vpn.transport.VpnTransportChain
import dev.proflyder.home.feature.vpn.transport.openconnect.AndroidSystemVpnNetwork
import dev.proflyder.home.feature.vpn.transport.openconnect.ExternalOpenConnectTransport
import dev.proflyder.home.feature.vpn.transport.openconnect.SystemVpnNetwork
import dev.proflyder.home.feature.vpn.transport.wireguard.WireGuardTransport
import dev.proflyder.home.feature.vpn.tunnel.ChainVpnController
import dev.proflyder.home.feature.vpn.tunnel.SystemVpnPermission
import dev.proflyder.home.feature.vpn.tunnel.TunnelLauncher
import dev.proflyder.home.feature.vpn.tunnel.TunnelSwitch
import dev.proflyder.home.feature.vpn.tunnel.VpnPermission
import dev.proflyder.home.feature.vpn.tunnel.VpnStatusServiceStarter
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.IntoSet
import dev.zacsweers.metro.Multibinds

@ContributesTo(AppScope::class)
@BindingContainer
abstract class VpnBindings {
    @Multibinds(allowEmpty = false)
    internal abstract fun transports(): Set<VpnTransport>

    @Binds
    @IntoSet
    internal abstract fun wireGuard(impl: WireGuardTransport): VpnTransport

    @Binds
    @IntoSet
    internal abstract fun openConnect(impl: ExternalOpenConnectTransport): VpnTransport

    @Binds
    internal abstract fun controller(impl: ChainVpnController): VpnController

    @Binds
    internal abstract fun launcher(impl: ChainVpnController): TunnelLauncher

    @Binds
    internal abstract fun systemStarts(impl: VpnTransportChain): SystemVpnStarts

    @Binds
    internal abstract fun tunnelSwitch(impl: ChainVpnController): TunnelSwitch

    @Binds
    internal abstract fun settings(impl: DataStoreVpnSettingsStore): VpnSettingsStore

    @Binds
    internal abstract fun profiles(impl: RemoteVpnProfiles): VpnProfiles

    @Binds
    internal abstract fun environment(impl: AndroidNetworkEnvironment): NetworkEnvironment

    @Binds
    internal abstract fun permission(impl: SystemVpnPermission): VpnPermission

    @Binds
    internal abstract fun systemVpn(impl: AndroidSystemVpnNetwork): SystemVpnNetwork

    @Binds
    @IntoSet
    internal abstract fun routeCondition(impl: VpnRouteCondition): RouteCondition

    @Binds
    @IntoSet
    internal abstract fun controllerStart(impl: ChainVpnController): AppStartable

    @Binds
    @IntoSet
    internal abstract fun coordinatorStart(impl: AutoConnectCoordinator): AppStartable

    @Binds
    @IntoSet
    internal abstract fun statusServiceStart(impl: VpnStatusServiceStarter): AppStartable

    @Binds
    @IntoSet
    internal abstract fun profileSyncStart(impl: VpnProfileSync): AppStartable
}
