package dev.proflyder.home.feature.push

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/**
 * Sends registration and rename calls with the stable [PushDeviceIdentity].
 *
 * Kept apart from FCM and Firebase: this is the part whose contract matters
 * (one device row across token rotations, name changes without a new row), so
 * it can be tested with a fake HTTP client and does not depend on Play
 * services.
 */
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
