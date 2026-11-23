package dev.proflyder.home.feature.call.presentation

import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.audio.CallAudioRoute
import dev.proflyder.home.feature.call.audio.CallAudioRouteState

internal sealed interface CallStatus {
    data object Dialing : CallStatus
    data object Ringing : CallStatus
    data object Connecting : CallStatus
    data class Active(val elapsedSeconds: Long) : CallStatus
    data class Ended(val reason: IntercomEndReason?, val failure: CallFailure?) : CallStatus
}

internal data class CallScreenState(
    val status: CallStatus = CallStatus.Dialing,
    val live: Boolean = false,
    val muted: Boolean = false,
    val routes: CallAudioRouteState = CallAudioRouteState(),
    val routeSheetVisible: Boolean = false,
    val proximityLock: Boolean = false,
)

internal data class CallScreenFlags(
    val opened: Boolean = false,
    val awaitingStart: Boolean = false,
    val routeSheetVisible: Boolean = false,
)

internal sealed interface CallIntent {
    data class Open(val startCall: Boolean) : CallIntent
    data class PermissionsResult(val granted: Boolean) : CallIntent
    data object ToggleMute : CallIntent
    data object HangupOrClose : CallIntent
    data object Back : CallIntent
    data object ShowRoutes : CallIntent
    data object HideRoutes : CallIntent
    data class SelectRoute(val route: CallAudioRoute) : CallIntent
}

internal sealed interface CallEvent {
    data class RequestPermissions(val permissions: List<String>) : CallEvent
    data object Finish : CallEvent
    data object MoveToBack : CallEvent
}

internal fun callScreenState(
    call: CallSnapshot,
    routes: CallAudioRouteState,
    flags: CallScreenFlags,
    elapsedSeconds: Long,
): CallScreenState = CallScreenState(
    status = call.toStatus(elapsedSeconds),
    live = call.isLive,
    muted = call.muted,
    routes = routes,
    routeSheetVisible = flags.routeSheetVisible && call.isLive,
    proximityLock = call.phase == CallPhase.ACTIVE && routes.selected == CallAudioRoute.EARPIECE,
)

internal fun shouldClose(call: CallSnapshot, flags: CallScreenFlags): Boolean =
    flags.opened && call.phase == CallPhase.IDLE && !flags.awaitingStart

private fun CallSnapshot.toStatus(elapsedSeconds: Long): CallStatus = when (phase) {
    CallPhase.IDLE, CallPhase.DIALING -> CallStatus.Dialing
    CallPhase.RINGING -> CallStatus.Ringing
    CallPhase.CONNECTING -> CallStatus.Connecting
    CallPhase.ACTIVE -> CallStatus.Active(elapsedSeconds)
    CallPhase.ENDED -> CallStatus.Ended(endReason, failure)
}
