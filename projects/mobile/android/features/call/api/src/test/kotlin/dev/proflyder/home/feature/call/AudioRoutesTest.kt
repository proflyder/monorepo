package dev.proflyder.home.feature.call

import org.junit.Assert.assertEquals
import org.junit.Test

class AudioRoutesTest {
    @Test
    fun builtInRoutesAreAlwaysOffered() {
        assertEquals(
            listOf(CallAudioRoute.EARPIECE, CallAudioRoute.SPEAKER),
            audioRoutesFor(emptyList()).map { it.route },
        )
    }

    @Test
    fun wiredHeadsetIsAddedOnce() {
        val routes = audioRoutesFor(
            listOf(
                AudioDevice(AudioDeviceKind.EARPIECE),
                AudioDevice(AudioDeviceKind.WIRED_HEADPHONES),
                AudioDevice(AudioDeviceKind.WIRED_HEADSET),
            ),
        )
        assertEquals(
            listOf(CallAudioRoute.EARPIECE, CallAudioRoute.SPEAKER, CallAudioRoute.WIRED_HEADSET),
            routes.map { it.route },
        )
    }

    @Test
    fun bluetoothCarriesTheDeviceName() {
        val routes = audioRoutesFor(
            listOf(AudioDevice(AudioDeviceKind.BLUETOOTH_SCO, "Pixel Buds")),
        )
        val bluetooth = routes.single { it.route == CallAudioRoute.BLUETOOTH }
        assertEquals("Pixel Buds", bluetooth.label)
    }

    @Test
    fun a2dpAndScoDoNotDuplicateTheBluetoothRoute() {
        val routes = audioRoutesFor(
            listOf(
                AudioDevice(AudioDeviceKind.BLUETOOTH_A2DP, "Car"),
                AudioDevice(AudioDeviceKind.BLUETOOTH_SCO, "Car"),
            ),
        )
        assertEquals(1, routes.count { it.route == CallAudioRoute.BLUETOOTH })
    }

    @Test
    fun usbHeadsetCountsAsWired() {
        val routes = audioRoutesFor(listOf(AudioDevice(AudioDeviceKind.USB_HEADSET)))
        assertEquals(
            listOf(CallAudioRoute.EARPIECE, CallAudioRoute.SPEAKER, CallAudioRoute.WIRED_HEADSET),
            routes.map { it.route },
        )
    }

    @Test
    fun unknownDevicesDoNotAddRoutes() {
        val routes = audioRoutesFor(listOf(AudioDevice(AudioDeviceKind.UNKNOWN, "?")))
        assertEquals(
            listOf(CallAudioRoute.EARPIECE, CallAudioRoute.SPEAKER),
            routes.map { it.route },
        )
    }
}
