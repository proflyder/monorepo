package dev.proflyder.home.feature.call

import kotlinx.coroutines.flow.StateFlow

/** The four output routes the call screen exposes. */
enum class CallAudioRoute {
    EARPIECE,
    SPEAKER,
    WIRED_HEADSET,
    BLUETOOTH,
}

/**
 * Platform-neutral description of one audio output device. The Android layer
 * maps `AudioDeviceInfo.type` to [AudioDeviceKind]; the mapping below is pure
 * Kotlin and unit tested.
 */
enum class AudioDeviceKind {
    EARPIECE,
    SPEAKER,
    WIRED_HEADSET,
    WIRED_HEADPHONES,
    BLUETOOTH_SCO,
    BLUETOOTH_A2DP,
    USB_HEADSET,
    UNKNOWN,
}

data class AudioDevice(val kind: AudioDeviceKind, val name: String = "")

data class AudioRouteOption(
    val route: CallAudioRoute,
    /** Bluetooth device name, when the route is a specific headset. */
    val label: String? = null,
)

data class CallAudioRouteState(
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

/**
 * Maps the current output devices to the route list. Built-in routes are always
 * offered (they do not depend on a hot-plugged device); wired and Bluetooth
 * appear only while such a device is connected. One Bluetooth route covers both
 * SCO and A2DP, because a call always runs over SCO.
 */
fun audioRoutesFor(devices: List<AudioDevice>): List<AudioRouteOption> {
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

/**
 * Selection of the audio output. The implementation owns Android specifics
 * (AudioManager, Telecom `CallEndpoint` on API 34+, Bluetooth permissions).
 */
interface CallAudioController {
    val state: StateFlow<CallAudioRouteState>

    /** Starts/stops device observation around a live call. */
    fun start()

    fun stop()

    fun refresh()

    fun select(route: CallAudioRoute)
}
