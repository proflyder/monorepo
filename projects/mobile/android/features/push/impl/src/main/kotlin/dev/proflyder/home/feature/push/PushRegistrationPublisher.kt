package dev.proflyder.home.feature.push

import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.PushDevicesApi
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@Inject
class PushRegistrationPublisher(
    private val client: PushDevicesApi,
    private val identity: DeviceIdentity,
) {
    internal suspend fun publish(request: PushRegistrationRequest): Outcome<Unit> = client.register(
        deviceId = identity.deviceId(),
        token = request.token,
        appVersion = request.appVersion,
    )

    internal suspend fun unregister(token: String): Outcome<Unit> = client.unregister(token)
}
