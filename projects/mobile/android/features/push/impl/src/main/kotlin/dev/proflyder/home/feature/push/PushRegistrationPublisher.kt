package dev.proflyder.home.feature.push

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@Inject
class PushRegistrationPublisher(
    private val client: PushDevicesApi,
    private val identity: PushDeviceIdentity,
) {
    internal suspend fun publish(
        token: String,
        appVersion: String,
        deviceName: String,
    ): Outcome<Unit> = client.register(
        deviceId = identity.deviceId(),
        token = token,
        appVersion = appVersion,
        deviceName = deviceName,
    )

    internal suspend fun rename(deviceName: String): Outcome<Unit> =
        client.rename(deviceId = identity.deviceId(), deviceName = deviceName)
}
