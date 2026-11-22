package dev.proflyder.home.feature.call.presentation

import android.content.Context
import android.text.format.DateUtils
import androidx.annotation.StringRes
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.audio.AudioRouteOption
import dev.proflyder.home.feature.call.audio.CallAudioRoute
import dev.proflyder.home.feature.call.impl.R

internal class CallTexts(private val context: Context) {
    fun status(status: CallStatus): String = when (status) {
        CallStatus.Dialing -> context.getString(R.string.call_status_dialing)
        CallStatus.Ringing -> context.getString(R.string.call_status_ringing)
        CallStatus.Connecting -> context.getString(R.string.call_status_connecting)
        is CallStatus.Active -> context.getString(
            R.string.call_status_active_timer,
            DateUtils.formatElapsedTime(status.elapsedSeconds),
        )
        is CallStatus.Ended -> context.getString(R.string.call_status_ended_reason, context.getString(reasonRes(status.reason)))
    }

    fun phase(phase: CallPhase): String = context.getString(
        when (phase) {
            CallPhase.IDLE, CallPhase.DIALING -> R.string.call_status_dialing
            CallPhase.RINGING -> R.string.call_status_ringing
            CallPhase.CONNECTING -> R.string.call_status_connecting
            CallPhase.ACTIVE -> R.string.call_status_active
            CallPhase.ENDED -> R.string.call_status_ended
        },
    )

    fun failure(failure: CallFailure): String = context.getString(
        when (failure) {
            CallFailure.NotConfigured -> R.string.call_error_not_configured
            CallFailure.StationOffline -> R.string.call_error_station_offline
            CallFailure.Busy -> R.string.call_error_busy
            CallFailure.MediaFailure -> R.string.call_error_media
            CallFailure.Timeout -> R.string.call_error_timeout
            CallFailure.PermissionDenied -> R.string.call_error_permission
            is CallFailure.Remote -> R.string.call_error_remote
        },
    )

    fun route(option: AudioRouteOption, selected: Boolean): String {
        val label = routeLabel(option)
        return if (selected) context.getString(R.string.call_route_selected, label) else label
    }

    private fun routeLabel(option: AudioRouteOption): String = when (option.route) {
        CallAudioRoute.EARPIECE -> context.getString(R.string.call_route_earpiece)
        CallAudioRoute.SPEAKER -> context.getString(R.string.call_route_speaker)
        CallAudioRoute.WIRED_HEADSET -> context.getString(R.string.call_route_wired)
        CallAudioRoute.BLUETOOTH -> option.label
            ?.let { context.getString(R.string.call_route_bluetooth_named, it) }
            ?: context.getString(R.string.call_route_bluetooth)
    }

    @StringRes
    private fun reasonRes(reason: IntercomEndReason?): Int = when (reason) {
        IntercomEndReason.HANGUP -> R.string.call_reason_hangup
        IntercomEndReason.DECLINED -> R.string.call_reason_declined
        IntercomEndReason.BUSY -> R.string.call_reason_busy
        IntercomEndReason.CANCELED -> R.string.call_reason_canceled
        IntercomEndReason.TIMEOUT -> R.string.call_reason_timeout
        IntercomEndReason.UNREACHABLE -> R.string.call_reason_unreachable
        IntercomEndReason.ERROR -> R.string.call_reason_error
        else -> R.string.call_reason_unknown
    }
}
