package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.text.format.DateUtils
import android.text.format.Formatter
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnProtocol
import dev.proflyder.home.feature.vpn.impl.R

internal object VpnTexts {
    fun title(connection: VpnConnection): Int = when (connection) {
        VpnConnection.Disconnected -> R.string.vpn_state_disconnected
        is VpnConnection.Connecting -> connectingTitle(connection.protocol)
        is VpnConnection.Connected -> R.string.vpn_state_connected
        VpnConnection.FallbackRequired -> R.string.vpn_state_fallback
        VpnConnection.NotNeededAtHome -> R.string.vpn_state_home
        is VpnConnection.Failed -> R.string.vpn_state_failed
    }

    fun detail(context: Context, connection: VpnConnection, nowMillis: Long): String = when (connection) {
        VpnConnection.Disconnected -> context.getString(R.string.vpn_detail_disconnected)
        is VpnConnection.Connecting ->
            context.getString(R.string.vpn_detail_connecting, context.getString(protocol(connection.protocol)))
        is VpnConnection.Connected -> context.getString(
            R.string.vpn_detail_connected,
            context.getString(protocol(connection.protocol)),
            duration(nowMillis - connection.sinceMillis),
        )
        VpnConnection.FallbackRequired -> context.getString(R.string.vpn_detail_fallback)
        VpnConnection.NotNeededAtHome -> context.getString(R.string.vpn_detail_home)
        is VpnConnection.Failed -> context.getString(problem(connection.problem))
    }

    fun traffic(context: Context, connection: VpnConnection.Connected): String? = when (connection.protocol) {
        VpnProtocol.WIREGUARD -> context.getString(
            R.string.vpn_traffic,
            Formatter.formatShortFileSize(context, connection.rxBytes),
            Formatter.formatShortFileSize(context, connection.txBytes),
        )
        VpnProtocol.OPENCONNECT -> null
    }

    fun profile(context: Context, state: VpnProfileState): String = when (state) {
        VpnProfileState.Missing -> context.getString(R.string.vpn_profile_missing)
        VpnProfileState.Loading -> context.getString(R.string.vpn_profile_loading)
        is VpnProfileState.Ready -> ready(context, state.profile)
        is VpnProfileState.Failed -> failed(context, state)
    }

    fun duration(millis: Long): String = DateUtils.formatElapsedTime(millis.coerceAtLeast(0) / MILLIS_IN_SECOND)

    private fun ready(context: Context, profile: VpnProfile): String = context.getString(
        R.string.vpn_profile_ready,
        profile.wireguard.address,
        DateUtils.formatDateTime(context, profile.updatedUnix * MILLIS_IN_SECOND, DATE_FLAGS),
    )

    private fun failed(context: Context, state: VpnProfileState.Failed): String {
        val reason = context.getString(problem(state.problem))
        val saved = state.profile ?: return reason
        return reason + "\n" + context.getString(R.string.vpn_profile_stale, ready(context, saved))
    }

    private fun connectingTitle(protocol: VpnProtocol): Int = when (protocol) {
        VpnProtocol.WIREGUARD -> R.string.vpn_state_connecting
        VpnProtocol.OPENCONNECT -> R.string.vpn_state_connecting_external
    }

    fun protocol(protocol: VpnProtocol): Int = when (protocol) {
        VpnProtocol.WIREGUARD -> R.string.vpn_protocol_wireguard
        VpnProtocol.OPENCONNECT -> R.string.vpn_protocol_openconnect
    }

    private fun problem(problem: VpnConnectProblem): Int = when (problem) {
        VpnConnectProblem.NO_PROFILE -> R.string.vpn_problem_no_profile
        VpnConnectProblem.PERMISSION -> R.string.vpn_problem_permission
        VpnConnectProblem.TUNNEL -> R.string.vpn_problem_tunnel
        VpnConnectProblem.NO_EXTERNAL_CLIENT -> R.string.vpn_problem_no_external
    }

    fun problem(problem: VpnProfileProblem): Int = when (problem) {
        VpnProfileProblem.NOT_CONFIGURED -> R.string.vpn_problem_not_configured
        VpnProfileProblem.OFFLINE -> R.string.vpn_problem_offline
        VpnProfileProblem.UNAUTHORIZED -> R.string.vpn_problem_unauthorized
        VpnProfileProblem.INVALID -> R.string.vpn_problem_invalid
        VpnProfileProblem.CONFLICT -> R.string.vpn_problem_conflict
        VpnProfileProblem.LIMIT -> R.string.vpn_problem_limit
        VpnProfileProblem.UNAVAILABLE -> R.string.vpn_problem_unavailable
        VpnProfileProblem.SERVER -> R.string.vpn_problem_server
    }

    private const val MILLIS_IN_SECOND = 1_000L
    private const val DATE_FLAGS = DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_TIME
}
