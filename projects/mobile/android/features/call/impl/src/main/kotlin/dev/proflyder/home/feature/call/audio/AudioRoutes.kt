package dev.proflyder.home.feature.call.audio

import kotlinx.coroutines.flow.StateFlow

internal enum class CallAudioRoute {
    EARPIECE,
    SPEAKER,
    WIRED_HEADSET,
    BLUETOOTH,
}

internal enum class AudioDeviceKind {
    EARPIECE,
    SPEAKER,
    WIRED_HEADSET,
    WIRED_HEADPHONES,
    BLUETOOTH_SCO,
    BLUETOOTH_A2DP,
    USB_HEADSET,
    UNKNOWN,
}

internal data class AudioDevice(val kind: AudioDeviceKind, val name: String = "")

internal data class AudioRouteOption(
    val route: CallAudioRoute,
    val label: String? = null,
)

internal data class CallAudioRouteState(
    val available: List<AudioRouteOption> = defaultRoutes,
    val selected: CallAudioRoute = CallAudioRoute.EARPIECE,
) {
    companion object {
        val defaultRoutes: List<AudioRouteOption> = listOf(
            AudioRouteOption(CallAudioRoute.EARPIECE),
            AudioRouteOption(CallAudioRoute.SPEAKER),
        )
    }
}

internal fun audioRoutesFor(devices: List<AudioDevice>): List<AudioRouteOption> {
    val kinds = devices.map { it.kind }.toSet()
    val bluetoothName = devices.firstOrNull {
        it.kind == AudioDeviceKind.BLUETOOTH_SCO || it.kind == AudioDeviceKind.BLUETOOTH_A2DP
    }?.name?.takeIf { it.isNotBlank() }
    val routes = mutableListOf(
        AudioRouteOption(CallAudioRoute.EARPIECE),
        AudioRouteOption(CallAudioRoute.SPEAKER),
    )
    if (AudioDeviceKind.WIRED_HEADSET in kinds || AudioDeviceKind.WIRED_HEADPHONES in kinds || AudioDeviceKind.USB_HEADSET in kinds) {
        routes += AudioRouteOption(CallAudioRoute.WIRED_HEADSET)
    }
    if (AudioDeviceKind.BLUETOOTH_SCO in kinds || AudioDeviceKind.BLUETOOTH_A2DP in kinds) {
        routes += AudioRouteOption(CallAudioRoute.BLUETOOTH, bluetoothName)
    }
    return routes
}

internal interface CallAudioController {
    val state: StateFlow<CallAudioRouteState>

    fun start()

    fun stop()

    fun refresh()

    fun select(route: CallAudioRoute)
}
