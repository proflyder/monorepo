package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome

interface PushDevicesApi {
    suspend fun register(
        deviceId: String,
        token: String,
        appVersion: String,
        deviceName: String,
    ): Outcome<Unit>

    suspend fun rename(deviceId: String, deviceName: String): Outcome<Unit>

    suspend fun unregister(token: String): Outcome<Unit>
}
