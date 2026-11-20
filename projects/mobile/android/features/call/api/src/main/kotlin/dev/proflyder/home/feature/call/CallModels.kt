package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.RemoteFailure

enum class CallPhase {
    IDLE,
    DIALING,
    RINGING,
    CONNECTING,
    ACTIVE,
    ENDED,
}

sealed interface CallFailure {
    data object NotConfigured : CallFailure
    data object StationOffline : CallFailure
    data object Busy : CallFailure
    data object PermissionDenied : CallFailure
    data object MediaFailure : CallFailure
    data object Timeout : CallFailure
    data class Remote(val failure: RemoteFailure) : CallFailure
}

data class CallSnapshot(
    val phase: CallPhase = CallPhase.IDLE,
    val callId: String? = null,
    val peerName: String = "",
    val muted: Boolean = false,
    val endReason: IntercomEndReason? = null,
    val failure: CallFailure? = null,
    val incoming: Boolean = false,
    val activeSinceUnix: Long = 0,
) {
    val isLive: Boolean
        get() = phase == CallPhase.DIALING ||
            phase == CallPhase.RINGING ||
            phase == CallPhase.CONNECTING ||
            phase == CallPhase.ACTIVE

    val isEnded: Boolean get() = phase == CallPhase.ENDED
}
