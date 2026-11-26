package dev.proflyder.home.core.model

enum class HealthLevel { OK, UNKNOWN, WARNING, CRITICAL }

data class SystemSections(
    val host: HealthLevel,
    val guests: HealthLevel,
    val k8s: HealthLevel,
    val devices: HealthLevel,
)

data class SystemAlert(
    val name: String,
    val severity: HealthLevel,
    val summary: String,
    val startedAtUnix: Long?,
)

data class SystemSummary(
    val status: HealthLevel,
    val generatedAtUnix: Long,
    val sections: SystemSections,
    val alerts: List<SystemAlert>,
    val alertsAvailable: Boolean,
)

data class Usage(val used: Double?, val total: Double?) {
    val ratio: Double?
        get() = when {
            used == null || total == null || total <= 0.0 -> null
            else -> (used / total).coerceIn(0.0, 1.0)
        }
}

data class PveHost(
    val name: String,
    val up: Boolean,
    val cpuRatio: Double?,
    val memory: Usage,
    val disk: Usage,
    val tempCelsius: Double?,
    val uptimeSeconds: Double?,
)

enum class GuestKind { VM, CT }

data class PveGuest(
    val vmid: Int,
    val name: String,
    val kind: GuestKind,
    val running: Boolean,
    val onboot: Boolean,
    val level: HealthLevel,
    val cpuRatio: Double?,
    val memory: Usage,
    val disk: Usage,
    val uptimeSeconds: Double?,
)

data class SystemHosts(
    val hostLevel: HealthLevel,
    val guestsLevel: HealthLevel,
    val host: PveHost?,
    val guestsAvailable: Boolean,
    val guests: List<PveGuest>,
    val targetsAvailable: Boolean = false,
    val targets: List<ScrapeTarget> = emptyList(),
)

data class ScrapeTarget(
    val job: String,
    val up: Int,
    val total: Int,
    val level: HealthLevel,
)

data class KubePod(
    val name: String,
    val phase: String,
    val ready: Int,
    val containers: Int,
    val restarts: Int,
    val recentRestarts: Int,
    val reason: String,
    val level: HealthLevel,
    val cpuCores: Double?,
    val memBytes: Double?,
    val ageSeconds: Double?,
)

data class KubeNamespace(
    val name: String,
    val level: HealthLevel,
    val pods: List<KubePod>,
)

data class KubeCluster(
    val level: HealthLevel,
    val available: Boolean,
    val podCount: Int,
    val problemCount: Int,
    val namespaces: List<KubeNamespace>,
)

data class StationHealth(
    val up: Boolean,
    val level: HealthLevel,
    val version: String,
    val model: String,
    val latencyMs: Double?,
    val wifiRssiDbm: Double?,
    val tempCelsius: Double?,
    val uptimeSeconds: Double?,
)

data class PhoneDevice(val id: String, val name: String, val platform: String)

data class SystemDevices(
    val level: HealthLevel,
    val station: StationHealth?,
    val phonesAvailable: Boolean,
    val phones: List<PhoneDevice>,
)

data class SystemSnapshot(
    val summary: SystemSummary,
    val hosts: SystemHosts,
    val cluster: KubeCluster,
    val devices: SystemDevices,
)
