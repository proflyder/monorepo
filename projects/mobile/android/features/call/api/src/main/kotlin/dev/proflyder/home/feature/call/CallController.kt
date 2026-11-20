package dev.proflyder.home.feature.call

import kotlinx.coroutines.flow.StateFlow

interface CallController {
    val state: StateFlow<CallSnapshot>

    fun placeCall()

    fun hangup()

    fun reportPermissionDenied()

    fun setMuted(muted: Boolean)

    fun dismissEnded()

    fun acceptIncoming(callId: String)

    fun declineIncoming(callId: String)
}

interface CallCoordinator {
    fun startOutgoingCall()

    fun hangup()
}

interface CallLauncher {
    fun openCallScreen()
}

interface CallNotificationController {
    fun start()
}
