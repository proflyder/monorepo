package dev.proflyder.home.feature.call

import android.content.pm.ServiceInfo
import android.os.Build
import org.junit.Assert.assertEquals
import org.junit.Test

internal class CallServiceTypesTest {
    @Test
    fun theMicrophoneTypeKeepsTheCallAudibleWhenTheScreenGoesOff() {
        val types = CallForegroundService.callServiceTypes(Build.VERSION_CODES.UPSIDE_DOWN_CAKE, microphoneGranted = true)
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE, types)
    }

    @Test
    fun withoutTheMicrophonePermissionOnlyThePhoneCallTypeIsClaimed() {
        val types = CallForegroundService.callServiceTypes(Build.VERSION_CODES.UPSIDE_DOWN_CAKE, microphoneGranted = false)
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL, types)
    }

    @Test
    fun beforeAndroidElevenTheMicrophoneTypeDoesNotExist() {
        val types = CallForegroundService.callServiceTypes(Build.VERSION_CODES.Q, microphoneGranted = true)
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL, types)
    }
}
