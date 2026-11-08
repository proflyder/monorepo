package dev.proflyder.home.feature.call.telecom

import android.net.Uri
import android.os.Build
import android.telecom.CallEndpoint
import android.telecom.Connection
import android.telecom.DisconnectCause
import android.telecom.TelecomManager
import androidx.annotation.RequiresApi
import dev.proflyder.home.core.model.IntercomEndReason
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * Self-managed Telecom connection for the station call.
 *
 * Telecom owns the call session (audio focus, system call UI, lock-screen
 * surface); the app owns the media and tells Telecom when the call becomes
 * active or ends by observing [CallController.state].
 */
class HomeConnection(
    private val controller: CallController,
    private val bridge: CallTelecomBridge,
    private val scope: CoroutineScope,
    private val callerName: String,
) : Connection() {

    private var observation: Job? = null

    init {
        connectionProperties = PROPERTY_SELF_MANAGED
        setAudioModeIsVoip(true)
        setInitializing()
        setAddress(Uri.fromParts("tel", "home", null), TelecomManager.PRESENTATION_ALLOWED)
        setCallerDisplayName(callerName, TelecomManager.PRESENTATION_ALLOWED)
        bridge.attach(this)
    }

    /** Called by the ConnectionService: start dialing and mirror the state. */
    fun startOutgoing() {
        setInitialized()
        controller.placeCall()
        observation = scope.launch {
            controller.state.collect(::render)
        }
    }

    private fun render(state: CallSnapshot) {
        when (state.phase) {
            CallPhase.IDLE -> Unit
            CallPhase.DIALING, CallPhase.RINGING, CallPhase.CONNECTING -> setDialing()
            CallPhase.ACTIVE -> setActive()
            CallPhase.ENDED -> {
                setDisconnected(state.toDisconnectCause())
                cleanup()
                destroy()
            }
        }
    }

    private fun CallSnapshot.toDisconnectCause(): DisconnectCause = when {
        failure == CallFailure.Busy -> DisconnectCause(DisconnectCause.BUSY)
        failure == CallFailure.PermissionDenied -> DisconnectCause(DisconnectCause.ERROR)
        failure == CallFailure.StationOffline -> DisconnectCause(DisconnectCause.REMOTE)
        endReason == IntercomEndReason.CANCELED -> DisconnectCause(DisconnectCause.CANCELED)
        endReason == IntercomEndReason.DECLINED -> DisconnectCause(DisconnectCause.REJECTED)
        endReason == IntercomEndReason.TIMEOUT -> DisconnectCause(DisconnectCause.ERROR)
        endReason == IntercomEndReason.UNREACHABLE -> DisconnectCause(DisconnectCause.REMOTE)
        endReason == IntercomEndReason.ERROR -> DisconnectCause(DisconnectCause.ERROR)
        else -> DisconnectCause(DisconnectCause.LOCAL)
    }

    override fun onDisconnect() {
        controller.hangup()
        cleanup()
        destroy()
    }

    override fun onReject() {
        controller.hangup()
        cleanup()
        destroy()
    }

    override fun onAbort() {
        controller.hangup()
        cleanup()
        destroy()
    }

    private fun cleanup() {
        observation?.cancel()
        observation = null
        bridge.detach(this)
    }

    override fun onHold() {
        setOnHold()
    }

    override fun onUnhold() {
        setActive()
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    override fun onAvailableCallEndpointsChanged(availableEndpoints: MutableList<CallEndpoint>) {
        bridge.rememberEndpoints(availableEndpoints)
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    override fun onCallEndpointChanged(endpoint: CallEndpoint) {
        bridge.onSelectedEndpoint(endpoint)
    }
}
