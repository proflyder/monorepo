package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.HomeNetworkState

internal enum class AutoConnectAction {
    NONE,
    CONNECT,
    ASK_PERMISSION,
    DISCONNECT,
}

internal enum class AutoConnectReason {
    USER_CHOICE,
    ENVIRONMENT_UNKNOWN,
    OFFLINE,
    AT_HOME,
    AT_HOME_AUTO_OFF,
    TUNNEL_ACTIVE,
    NO_PROFILE,
    NOT_REQUESTED,
    AWAY_REQUESTED,
    PERMISSION_ASK,
    PERMISSION_MISSING,
}

internal data class AutoConnectDecision(val action: AutoConnectAction, val reason: AutoConnectReason)

internal data class AutoConnectInput(
    val foreground: Boolean,
    val autoConnect: Boolean,
    val connectOnOpen: Boolean,
    val environment: HomeNetworkState,
    val active: Boolean,
    val hasProfile: Boolean,
    val permissionGranted: Boolean,
    val permissionAsked: Boolean,
    val userWish: TunnelWish? = null,
)

internal object AutoConnectPolicy {
    fun decide(input: AutoConnectInput): AutoConnectDecision {
        if (input.userWish != null) return idle(AutoConnectReason.USER_CHOICE)
        return when (input.environment) {
            HomeNetworkState.Unknown -> idle(AutoConnectReason.ENVIRONMENT_UNKNOWN)
            HomeNetworkState.Offline -> idle(AutoConnectReason.OFFLINE)
            is HomeNetworkState.Home -> atHome(input)
            is HomeNetworkState.Away -> away(input)
        }
    }

    private fun atHome(input: AutoConnectInput): AutoConnectDecision = when {
        input.autoConnect && input.active -> AutoConnectDecision(AutoConnectAction.DISCONNECT, AutoConnectReason.AT_HOME_AUTO_OFF)
        else -> idle(AutoConnectReason.AT_HOME)
    }

    private fun away(input: AutoConnectInput): AutoConnectDecision = when {
        input.active -> idle(AutoConnectReason.TUNNEL_ACTIVE)
        !input.hasProfile -> idle(AutoConnectReason.NO_PROFILE)
        !requested(input) -> idle(AutoConnectReason.NOT_REQUESTED)
        input.permissionGranted -> AutoConnectDecision(AutoConnectAction.CONNECT, AutoConnectReason.AWAY_REQUESTED)
        input.foreground && !input.permissionAsked -> AutoConnectDecision(AutoConnectAction.ASK_PERMISSION, AutoConnectReason.PERMISSION_ASK)
        else -> idle(AutoConnectReason.PERMISSION_MISSING)
    }

    private fun requested(input: AutoConnectInput): Boolean =
        input.autoConnect || input.foreground && input.connectOnOpen

    private fun idle(reason: AutoConnectReason) = AutoConnectDecision(AutoConnectAction.NONE, reason)
}
