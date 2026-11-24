package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.text.format.DateUtils
import android.text.format.Formatter
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.feature.vpn.VpnConnectProblem
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileProblem
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnTraffic
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.proflyder.home.feature.vpn.impl.R

internal object VpnTexts {
    fun title(connection: VpnConnection): Int = when (connection) {
        VpnConnection.Disconnected -> R.string.vpn_state_disconnected
        is VpnConnection.Connecting -> connectingTitle(connection.transport)
        is VpnConnection.Connected -> R.string.vpn_state_connected
        is VpnConnection.FallbackRequired -> R.string.vpn_state_fallback
        VpnConnection.NotNeededAtHome -> R.string.vpn_state_home
        is VpnConnection.Failed -> R.string.vpn_state_failed
    }

    fun detail(context: Context, connection: VpnConnection, nowMillis: Long): String = when (connection) {
        VpnConnection.Disconnected -> context.getString(R.string.vpn_detail_disconnected)
        is VpnConnection.Connecting ->
            context.getString(R.string.vpn_detail_connecting, context.getString(connection.transport.labelRes))
        is VpnConnection.Connected -> context.getString(
            connectedDetail(connection.verified),
            context.getString(connection.transport.labelRes),
            duration(nowMillis - connection.sinceMillis),
        )
        is VpnConnection.FallbackRequired -> context.getString(R.string.vpn_detail_fallback)
        VpnConnection.NotNeededAtHome -> context.getString(R.string.vpn_detail_home)
        is VpnConnection.Failed -> context.getString(problem(connection.problem))
    }

    fun traffic(context: Context, traffic: VpnTraffic): String = context.getString(
        R.string.vpn_traffic,
        Formatter.formatShortFileSize(context, traffic.rxBytes),
        Formatter.formatShortFileSize(context, traffic.txBytes),
    )

    fun profile(context: Context, state: VpnProfileState): String = when (state) {
        VpnProfileState.Missing -> context.getString(R.string.vpn_profile_missing)
        VpnProfileState.Loading -> context.getString(R.string.vpn_profile_loading)
        is VpnProfileState.Ready -> ready(context, state.profile)
        is VpnProfileState.Failed -> failed(context, state)
    }

    fun duration(millis: Long): String = DateUtils.formatElapsedTime(millis.coerceAtLeast(0) / MILLIS_IN_SECOND)

    private fun ready(context: Context, profile: VpnProfile): String = context.getString(
        R.string.vpn_profile_ready,
        profile.deviceName,
        DateUtils.formatDateTime(context, profile.updatedUnix * MILLIS_IN_SECOND, DATE_FLAGS),
    )

    private fun failed(context: Context, state: VpnProfileState.Failed): String {
        val reason = context.getString(problem(state.problem))
        val saved = state.profile ?: return reason
        return reason + "\n" + context.getString(R.string.vpn_profile_stale, ready(context, saved))
    }

    private fun connectingTitle(transport: VpnTransportInfo): Int = when (transport.external) {
        true -> R.string.vpn_state_connecting_external
        false -> R.string.vpn_state_connecting
    }

    private fun connectedDetail(verified: Boolean): Int = when (verified) {
        true -> R.string.vpn_detail_connected
        false -> R.string.vpn_detail_connected_unverified
    }

    private fun problem(problem: VpnConnectProblem): Int = when (problem) {
        VpnConnectProblem.NO_PROFILE -> R.string.vpn_problem_no_profile
        VpnConnectProblem.PERMISSION -> R.string.vpn_problem_permission
        VpnConnectProblem.TUNNEL -> R.string.vpn_problem_tunnel
        VpnConnectProblem.NO_CLIENT -> R.string.vpn_problem_no_external
    }

    fun problem(problem: VpnProfileProblem): Int = when (problem) {
        VpnProfileProblem.NOT_CONFIGURED -> R.string.vpn_problem_not_configured
        VpnProfileProblem.OFFLINE -> R.string.vpn_problem_offline
        VpnProfileProblem.UNAUTHORIZED -> R.string.vpn_problem_unauthorized
        VpnProfileProblem.NO_ACCESS -> R.string.vpn_problem_no_access
        VpnProfileProblem.INVALID -> R.string.vpn_problem_invalid
        VpnProfileProblem.CONFLICT -> R.string.vpn_problem_conflict
        VpnProfileProblem.LIMIT -> R.string.vpn_problem_limit
        VpnProfileProblem.UNAVAILABLE -> R.string.vpn_problem_unavailable
        VpnProfileProblem.SERVER -> R.string.vpn_problem_server
    }

    private const val MILLIS_IN_SECOND = 1_000L
    private const val DATE_FLAGS = DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_TIME
}
