package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.map
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class SchedulerServerClockSync(
    private val client: SchedulerClient,
) : ServerClockSync {
    override suspend fun sync(): Outcome<Unit> = client.getStatus().map { }
}
