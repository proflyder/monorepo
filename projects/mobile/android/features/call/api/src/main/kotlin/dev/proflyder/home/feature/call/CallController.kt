package dev.proflyder.home.feature.call

import kotlinx.coroutines.flow.StateFlow

/**
 * Single source of truth for the live call. UI, Telecom connection, foreground
 * service and notification observe [state] and never keep their own copy.
 */
interface CallController {
    val state: StateFlow<CallSnapshot>

    /** Dials the station. No-op when a call is already live. */
    fun placeCall()

    /** Ends the live call locally and on the server. */
    fun hangup()

    /** Shows a controlled "microphone permission denied" ended state before dialing. */
    fun reportPermissionDenied()

    fun setMuted(muted: Boolean)

    /** Returns the screen to IDLE after an ENDED snapshot has been shown. */
    fun dismissEnded()

    /** TODO(incoming): no incoming UI yet; these exist so the API is symmetric. */
    fun acceptIncoming(callId: String)

    fun declineIncoming(callId: String)
}

/**
 * Places the outgoing call through Android Telecom when possible, so the call
 * gets a self-managed `Connection`, audio focus and the system call UI. Falls
 * back to [CallController.placeCall] when Telecom is unavailable or the app is
 * not allowed to manage calls.
 */
interface CallCoordinator {
    fun startOutgoingCall()

    fun hangup()
}

/** Opens the in-call screen; used by the Home quick action. */
interface CallLauncher {
    fun openCallScreen()
}

/**
 * Observes [CallController] and runs the `phoneCall` foreground service with a
 * CallStyle ongoing notification ("Сбросить"). Started once from the app.
 */
interface CallNotificationController {
    fun start()
}
