package dev.proflyder.home.feature.system.data

import dev.proflyder.home.core.model.KubeCluster
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SystemDevices
import dev.proflyder.home.core.model.SystemHosts
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.model.SystemSummary
import dev.proflyder.home.core.network.SystemDashboardClient
import dev.proflyder.home.feature.system.SystemRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpSystemRepository(
    private val client: SystemDashboardClient,
) : SystemRepository {
    override suspend fun load(): Outcome<SystemSnapshot> = coroutineScope {
        val summary = async { client.summary() }
        val hosts = async { client.hosts() }
        val cluster = async { client.cluster() }
        val devices = async { client.devices() }
        combine(summary.await(), hosts.await(), cluster.await(), devices.await())
    }
}

internal fun combine(
    summary: Outcome<SystemSummary>,
    hosts: Outcome<SystemHosts>,
    cluster: Outcome<KubeCluster>,
    devices: Outcome<SystemDevices>,
): Outcome<SystemSnapshot> {
    val failure = listOf(summary, hosts, cluster, devices).firstNotNullOfOrNull { it.failureOrNull() }
    return when (failure) {
        null -> Outcome.Success(
            SystemSnapshot(
                summary = checkNotNull(summary.getOrNull()),
                hosts = checkNotNull(hosts.getOrNull()),
                cluster = checkNotNull(cluster.getOrNull()),
                devices = checkNotNull(devices.getOrNull()),
            ),
        )
        else -> Outcome.Failure(failure)
    }
}
