package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.KubeCluster
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SystemDevices
import dev.proflyder.home.core.model.SystemHosts
import dev.proflyder.home.core.model.SystemSummary

interface SystemDashboardClient {
    suspend fun summary(): Outcome<SystemSummary>
    suspend fun hosts(): Outcome<SystemHosts>
    suspend fun cluster(): Outcome<KubeCluster>
    suspend fun devices(): Outcome<SystemDevices>
}
