package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.GuestKind
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.model.KubeCluster
import dev.proflyder.home.core.model.KubeNamespace
import dev.proflyder.home.core.model.KubePod
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PhoneDevice
import dev.proflyder.home.core.model.PveGuest
import dev.proflyder.home.core.model.PveHost
import dev.proflyder.home.core.model.StationHealth
import dev.proflyder.home.core.model.SystemAlert
import dev.proflyder.home.core.model.SystemDevices
import dev.proflyder.home.core.model.SystemHosts
import dev.proflyder.home.core.model.SystemSections
import dev.proflyder.home.core.model.SystemSummary
import dev.proflyder.home.core.model.Usage
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import retrofit2.http.GET

interface SystemDashboardClient {
    suspend fun summary(): Outcome<SystemSummary>
    suspend fun hosts(): Outcome<SystemHosts>
    suspend fun cluster(): Outcome<KubeCluster>
    suspend fun devices(): Outcome<SystemDevices>
}

@Serializable
internal data class SystemSectionsDto(
    val host: String = "",
    val guests: String = "",
    val k8s: String = "",
    val devices: String = "",
)

@Serializable
internal data class SystemAlertDto(
    val name: String = "",
    val severity: String = "",
    val summary: String = "",
    val startedAt: Long? = null,
)

@Serializable
internal data class SystemSummaryDto(
    val status: String = "",
    val generatedAt: Long = 0,
    val sections: SystemSectionsDto = SystemSectionsDto(),
    val alerts: List<SystemAlertDto> = emptyList(),
    val alertsAvailable: Boolean = false,
)

@Serializable
internal data class PveHostDto(
    val name: String = "",
    val up: Boolean = false,
    val cpuRatio: Double? = null,
    val memUsedBytes: Double? = null,
    val memTotalBytes: Double? = null,
    val diskUsedBytes: Double? = null,
    val diskTotalBytes: Double? = null,
    val tempCelsius: Double? = null,
    val uptimeSeconds: Double? = null,
)

@Serializable
internal data class PveGuestDto(
    val vmid: Int = 0,
    val name: String = "",
    val kind: String = "",
    val running: Boolean = false,
    val onboot: Boolean = false,
    val status: String = "",
    val cpuRatio: Double? = null,
    val memUsedBytes: Double? = null,
    val memTotalBytes: Double? = null,
    val diskUsedBytes: Double? = null,
    val diskTotalBytes: Double? = null,
    val uptimeSeconds: Double? = null,
)

@Serializable
internal data class SystemHostsDto(
    val status: String = "",
    val guestsStatus: String = "",
    val host: PveHostDto? = null,
    val guestsAvailable: Boolean = false,
    val guests: List<PveGuestDto> = emptyList(),
)

@Serializable
internal data class KubePodDto(
    val name: String = "",
    val phase: String = "",
    val ready: Int = 0,
    val containers: Int = 0,
    val restarts: Int = 0,
    val recentRestarts: Int = 0,
    val reason: String = "",
    val status: String = "",
    val cpuCores: Double? = null,
    val memBytes: Double? = null,
    val ageSeconds: Double? = null,
)

@Serializable
internal data class KubeNamespaceDto(
    val name: String = "",
    val status: String = "",
    val pods: List<KubePodDto> = emptyList(),
)

@Serializable
internal data class KubeClusterDto(
    val status: String = "",
    val available: Boolean = false,
    val podCount: Int = 0,
    val problemCount: Int = 0,
    val namespaces: List<KubeNamespaceDto> = emptyList(),
)

@Serializable
internal data class StationDto(
    val up: Boolean = false,
    val status: String = "",
    val version: String = "",
    val model: String = "",
    val latencyMs: Double? = null,
    val wifiRssiDbm: Double? = null,
    val tempCelsius: Double? = null,
    val uptimeSeconds: Double? = null,
)

@Serializable
internal data class PhoneDto(val id: String = "", val name: String = "", val platform: String = "")

@Serializable
internal data class SystemDevicesDto(
    val status: String = "",
    val station: StationDto? = null,
    val phonesAvailable: Boolean = false,
    val phones: List<PhoneDto> = emptyList(),
)

internal interface SystemApi {
    @GET("v1/system/summary")
    fun summary(): Outcome<SystemSummaryDto>

    @GET("v1/system/hosts")
    fun hosts(): Outcome<SystemHostsDto>

    @GET("v1/system/k8s")
    fun cluster(): Outcome<KubeClusterDto>

    @GET("v1/system/devices")
    fun devices(): Outcome<SystemDevicesDto>
}

internal fun String.toHealthLevel(): HealthLevel = when (this) {
    "ok" -> HealthLevel.OK
    "warning" -> HealthLevel.WARNING
    "critical" -> HealthLevel.CRITICAL
    else -> HealthLevel.UNKNOWN
}

internal fun severityLevel(severity: String): HealthLevel = when (severity) {
    "critical" -> HealthLevel.CRITICAL
    "warning" -> HealthLevel.WARNING
    else -> HealthLevel.UNKNOWN
}

internal fun SystemSummaryDto.toDomain(): SystemSummary = SystemSummary(
    status = status.toHealthLevel(),
    generatedAtUnix = generatedAt,
    sections = SystemSections(
        host = sections.host.toHealthLevel(),
        guests = sections.guests.toHealthLevel(),
        k8s = sections.k8s.toHealthLevel(),
        devices = sections.devices.toHealthLevel(),
    ),
    alerts = alerts.map { SystemAlert(it.name, severityLevel(it.severity), it.summary, it.startedAt) },
    alertsAvailable = alertsAvailable,
)

internal fun PveHostDto.toDomain(): PveHost = PveHost(
    name = name,
    up = up,
    cpuRatio = cpuRatio,
    memory = Usage(memUsedBytes, memTotalBytes),
    disk = Usage(diskUsedBytes, diskTotalBytes),
    tempCelsius = tempCelsius,
    uptimeSeconds = uptimeSeconds,
)

internal fun PveGuestDto.toDomain(): PveGuest = PveGuest(
    vmid = vmid,
    name = name.ifEmpty { vmid.toString() },
    kind = if (kind == "ct") GuestKind.CT else GuestKind.VM,
    running = running,
    onboot = onboot,
    level = status.toHealthLevel(),
    cpuRatio = cpuRatio,
    memory = Usage(memUsedBytes, memTotalBytes),
    disk = Usage(diskUsedBytes, diskTotalBytes),
    uptimeSeconds = uptimeSeconds,
)

internal fun SystemHostsDto.toDomain(): SystemHosts = SystemHosts(
    hostLevel = status.toHealthLevel(),
    guestsLevel = guestsStatus.toHealthLevel(),
    host = host?.toDomain(),
    guestsAvailable = guestsAvailable,
    guests = guests.map { it.toDomain() },
)

internal fun KubePodDto.toDomain(): KubePod = KubePod(
    name = name,
    phase = phase,
    ready = ready,
    containers = containers,
    restarts = restarts,
    recentRestarts = recentRestarts,
    reason = reason,
    level = status.toHealthLevel(),
    cpuCores = cpuCores,
    memBytes = memBytes,
    ageSeconds = ageSeconds,
)

internal fun KubeClusterDto.toDomain(): KubeCluster = KubeCluster(
    level = status.toHealthLevel(),
    available = available,
    podCount = podCount,
    problemCount = problemCount,
    namespaces = namespaces.map { ns -> KubeNamespace(ns.name, ns.status.toHealthLevel(), ns.pods.map { it.toDomain() }) },
)

internal fun StationDto.toDomain(): StationHealth = StationHealth(
    up = up,
    level = status.toHealthLevel(),
    version = version,
    model = model,
    latencyMs = latencyMs,
    wifiRssiDbm = wifiRssiDbm,
    tempCelsius = tempCelsius,
    uptimeSeconds = uptimeSeconds,
)

internal fun SystemDevicesDto.toDomain(): SystemDevices = SystemDevices(
    level = status.toHealthLevel(),
    station = station?.toDomain(),
    phonesAvailable = phonesAvailable,
    phones = phones.map { PhoneDevice(it.id, it.name, it.platform) },
)

class HttpSystemDashboardClient(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
    interceptors: List<Interceptor> = emptyList(),
    networkWaiter: NetworkWaiter = NetworkWaiter.Online,
) : SystemDashboardClient {
    private val api: SystemApi = HomeRetrofit.create(
        SystemApi::class.java,
        settings,
        baseClient,
        Target.CONTROL,
        interceptors,
        networkWaiter,
    )

    override suspend fun summary(): Outcome<SystemSummary> =
        withContext(ioDispatcher) { api.summary().mapResult { it.toDomain() } }

    override suspend fun hosts(): Outcome<SystemHosts> =
        withContext(ioDispatcher) { api.hosts().mapResult { it.toDomain() } }

    override suspend fun cluster(): Outcome<KubeCluster> =
        withContext(ioDispatcher) { api.cluster().mapResult { it.toDomain() } }

    override suspend fun devices(): Outcome<SystemDevices> =
        withContext(ioDispatcher) { api.devices().mapResult { it.toDomain() } }
}
