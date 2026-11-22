package dev.proflyder.home.feature.vpn.connect

import dev.proflyder.home.feature.vpn.HomeNetworkState
import dev.proflyder.home.feature.vpn.atHome

internal enum class AutoConnectAction {
    NONE,
    CONNECT,
    ASK_PERMISSION,
    DISCONNECT,
}

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
    fun decide(input: AutoConnectInput): AutoConnectAction {
        if (input.userWish != null) return AutoConnectAction.NONE
        val atHome = input.environment.atHome ?: return AutoConnectAction.NONE
        return when {
            atHome -> atHomeAction(input)
            !wanted(input) -> AutoConnectAction.NONE
            input.permissionGranted -> AutoConnectAction.CONNECT
            input.foreground && !input.permissionAsked -> AutoConnectAction.ASK_PERMISSION
            else -> AutoConnectAction.NONE
        }
    }

    private fun atHomeAction(input: AutoConnectInput): AutoConnectAction = when {
        input.autoConnect && input.active -> AutoConnectAction.DISCONNECT
        else -> AutoConnectAction.NONE
    }

    private fun wanted(input: AutoConnectInput): Boolean =
        !input.active && input.hasProfile && (input.autoConnect || input.foreground && input.connectOnOpen)
}
