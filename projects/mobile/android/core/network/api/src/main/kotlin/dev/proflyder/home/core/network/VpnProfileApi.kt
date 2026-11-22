package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VpnProfile

interface VpnProfileApi {
    suspend fun register(deviceId: String, publicKey: String, deviceName: String): Outcome<VpnProfile>

    suspend fun remove(deviceId: String): Outcome<Boolean>
}
