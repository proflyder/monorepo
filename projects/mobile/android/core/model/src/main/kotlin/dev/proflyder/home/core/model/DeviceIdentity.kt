package dev.proflyder.home.core.model

interface DeviceIdentity {
    suspend fun deviceId(): String

    fun defaultName(): String
}
