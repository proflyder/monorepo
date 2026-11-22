package dev.proflyder.home.feature.call

import kotlinx.coroutines.flow.StateFlow

interface CallController {
    val state: StateFlow<CallSnapshot>

    fun placeCall()

    fun hangup()

    fun reportPermissionDenied()

    fun setMuted(muted: Boolean)

    fun dismissEnded()
}

interface CallLauncher {
    fun openCallScreen()
}
