package dev.proflyder.home.feature.call

import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.core.model.RemoteFailure

/** High-level phase of the phone-side call. */
enum class CallPhase {
    IDLE,
    /** StartCall has not returned yet. */
    DIALING,
    /** The station is ringing. */
    RINGING,
    /** The station answered; the offer/answer exchange is running. */
    CONNECTING,
    ACTIVE,
    ENDED,
}

/** User-facing failure; distinguishes "the call did not start" from media errors. */
sealed interface CallFailure {
    data object NotConfigured : CallFailure
    data object StationOffline : CallFailure
    data object Busy : CallFailure
    data object PermissionDenied : CallFailure
    data object MediaFailure : CallFailure
    data object Timeout : CallFailure
    data class Remote(val failure: RemoteFailure) : CallFailure
}

/**
 * Everything the call screen, the Telecom connection and the notification need.
 * One immutable snapshot is the single source of truth for all three.
 */
data class CallSnapshot(
    val phase: CallPhase = CallPhase.IDLE,
    val callId: String? = null,
    val peerName: String = "",
    val muted: Boolean = false,
    val endReason: IntercomEndReason? = null,
    val failure: CallFailure? = null,
    /** True only for a remote incoming call; the UI currently declines those. */
    val incoming: Boolean = false,
    /** Set when the call reached ACTIVE, so the screen can show a timer. */
    val activeSinceUnix: Long = 0,
) {
    val isLive: Boolean
        get() = phase == CallPhase.DIALING ||
            phase == CallPhase.RINGING ||
            phase == CallPhase.CONNECTING ||
            phase == CallPhase.ACTIVE

    val isEnded: Boolean get() = phase == CallPhase.ENDED
}
