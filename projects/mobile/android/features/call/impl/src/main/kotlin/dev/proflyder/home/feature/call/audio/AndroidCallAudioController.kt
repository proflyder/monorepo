package dev.proflyder.home.feature.call.audio

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.telecom.CallEndpoint
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import dev.proflyder.home.feature.call.AudioDevice
import dev.proflyder.home.feature.call.AudioDeviceKind
import dev.proflyder.home.feature.call.CallAudioController
import dev.proflyder.home.feature.call.CallAudioRoute
import dev.proflyder.home.feature.call.CallAudioRouteState
import dev.proflyder.home.feature.call.audioRoutesFor
import dev.proflyder.home.feature.call.telecom.CallTelecomBridge
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Routes call audio.
 *
 * API 34+ prefers Telecom `CallEndpoint` selection (the self-managed connection
 * is the owner of the call). Everywhere else — or when Telecom cannot honour the
 * request — it falls back to `AudioManager.setCommunicationDevice` (API 31+)
 * and, on older releases, speakerphone/Bluetooth SCO flags.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidCallAudioController(
    private val context: Context,
    private val telecomBridge: CallTelecomBridge,
    private val scope: CoroutineScope,
) : CallAudioController {

    private val audioManager = context.getSystemService(AudioManager::class.java)
    private val mutableState = MutableStateFlow(CallAudioRouteState())
    override val state: StateFlow<CallAudioRouteState> = mutableState.asStateFlow()

    private var observing = false
    private var previousMode: Int? = null
    private var telecomJob: Job? = null

    private val deviceCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>) = refresh()
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) = refresh()
    }

    override fun start() {
        if (observing) return
        observing = true
        previousMode = audioManager?.mode
        audioManager?.mode = AudioManager.MODE_IN_COMMUNICATION
        audioManager?.registerAudioDeviceCallback(deviceCallback, Handler(Looper.getMainLooper()))
        refresh()
        telecomJob = scope.launch {
            telecomBridge.selectedEndpointType.collect { type ->
                type?.let { selected ->
                    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) return@let
                    val route = endpointTypeToRoute(selected) ?: return@let
                    mutableState.value = mutableState.value.copy(selected = route)
                }
            }
        }
    }

    override fun stop() {
        if (!observing) return
        observing = false
        telecomJob?.cancel()
        telecomJob = null
        audioManager?.unregisterAudioDeviceCallback(deviceCallback)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            audioManager?.clearCommunicationDevice()
        } else {
            @Suppress("DEPRECATION")
            audioManager?.isSpeakerphoneOn = false
        }
        audioManager?.mode = previousMode ?: AudioManager.MODE_NORMAL
        previousMode = null
    }

    override fun refresh() {
        val devices = outputDevices()
        val routes = audioRoutesFor(devices.map { AudioDevice(it.kind(), it.productName?.toString().orEmpty()) })
        val selected = mutableState.value.selected
            .takeIf { route -> routes.any { it.route == route } }
            ?: CallAudioRoute.EARPIECE
        mutableState.value = mutableState.value.copy(available = routes, selected = selected)
    }

    override fun select(route: CallAudioRoute) {
        if (mutableState.value.available.none { it.route == route }) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE && selectThroughTelecom(route)) {
            return
        }
        applyThroughAudioManager(route)
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun selectThroughTelecom(route: CallAudioRoute): Boolean {
        val endpointType = routeToEndpointType(route) ?: return false
        val requested = telecomBridge.requestEndpoint(
            endpointType,
            ContextCompat.getMainExecutor(context),
        ) { success ->
            if (success) {
                mutableState.value = mutableState.value.copy(selected = route)
            } else {
                applyThroughAudioManager(route)
            }
        }
        return requested
    }

    private fun applyThroughAudioManager(route: CallAudioRoute) {
        val manager = audioManager ?: return
        val devices = outputDevices()
        val target = when (route) {
            CallAudioRoute.EARPIECE -> devices.firstOrNull { it.kind() == AudioDeviceKind.EARPIECE }
            CallAudioRoute.SPEAKER -> devices.firstOrNull { it.kind() == AudioDeviceKind.SPEAKER }
            CallAudioRoute.WIRED_HEADSET -> devices.firstOrNull {
                it.kind() == AudioDeviceKind.WIRED_HEADSET ||
                    it.kind() == AudioDeviceKind.WIRED_HEADPHONES ||
                    it.kind() == AudioDeviceKind.USB_HEADSET
            }
            // A call always uses SCO; prefer it over a media-only A2DP device.
            CallAudioRoute.BLUETOOTH -> devices.firstOrNull { it.kind() == AudioDeviceKind.BLUETOOTH_SCO }
                ?: devices.firstOrNull { it.kind() == AudioDeviceKind.BLUETOOTH_A2DP }
        } ?: return

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            manager.setCommunicationDevice(target)
        } else {
            @Suppress("DEPRECATION")
            when (route) {
                CallAudioRoute.SPEAKER -> manager.isSpeakerphoneOn = true
                CallAudioRoute.EARPIECE, CallAudioRoute.WIRED_HEADSET -> manager.isSpeakerphoneOn = false
                CallAudioRoute.BLUETOOTH -> {
                    manager.startBluetoothSco()
                    manager.isBluetoothScoOn = true
                }
            }
        }
        mutableState.value = mutableState.value.copy(selected = route)
    }

    private fun outputDevices(): List<AudioDeviceInfo> {
        val manager = audioManager ?: return emptyList()
        return manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS).filter { device ->
            if (device.kind() == AudioDeviceKind.BLUETOOTH_SCO || device.kind() == AudioDeviceKind.BLUETOOTH_A2DP) {
                hasBluetoothPermission()
            } else {
                true
            }
        }
    }

    private fun hasBluetoothPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
            PackageManager.PERMISSION_GRANTED

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun routeToEndpointType(route: CallAudioRoute): Int? = when (route) {
        CallAudioRoute.EARPIECE -> CallEndpoint.TYPE_EARPIECE
        CallAudioRoute.SPEAKER -> CallEndpoint.TYPE_SPEAKER
        CallAudioRoute.WIRED_HEADSET -> CallEndpoint.TYPE_WIRED_HEADSET
        CallAudioRoute.BLUETOOTH -> CallEndpoint.TYPE_BLUETOOTH
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun endpointTypeToRoute(type: Int): CallAudioRoute? = when (type) {
        CallEndpoint.TYPE_EARPIECE -> CallAudioRoute.EARPIECE
        CallEndpoint.TYPE_SPEAKER -> CallAudioRoute.SPEAKER
        CallEndpoint.TYPE_WIRED_HEADSET -> CallAudioRoute.WIRED_HEADSET
        CallEndpoint.TYPE_BLUETOOTH -> CallAudioRoute.BLUETOOTH
        else -> null
    }
}

internal fun AudioDeviceInfo.kind(): AudioDeviceKind = when (type) {
    AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> AudioDeviceKind.EARPIECE
    AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> AudioDeviceKind.SPEAKER
    AudioDeviceInfo.TYPE_WIRED_HEADSET -> AudioDeviceKind.WIRED_HEADSET
    AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> AudioDeviceKind.WIRED_HEADPHONES
    AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> AudioDeviceKind.BLUETOOTH_SCO
    AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> AudioDeviceKind.BLUETOOTH_A2DP
    AudioDeviceInfo.TYPE_USB_HEADSET -> AudioDeviceKind.USB_HEADSET
    else -> AudioDeviceKind.UNKNOWN
}
