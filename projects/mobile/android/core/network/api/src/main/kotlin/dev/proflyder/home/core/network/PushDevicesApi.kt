package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome

interface PushDevicesApi {
    suspend fun register(
        deviceId: String,
        token: String,
        appVersion: String,
    ): Outcome<Unit>

    suspend fun unregister(token: String): Outcome<Unit>
}
