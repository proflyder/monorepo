package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.HermesAgent
import dev.proflyder.home.core.model.HermesAgentStatus
import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.core.model.HermesReport
import dev.proflyder.home.core.model.HermesReports
import dev.proflyder.home.core.model.HermesSkill
import dev.proflyder.home.core.model.HermesSkills
import dev.proflyder.home.core.model.HermesToolset
import dev.proflyder.home.core.model.HermesToolsets
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.VoiceRequest
import dev.proflyder.home.core.model.VoiceStatus
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.serialization.Serializable
import retrofit2.http.GET

@Serializable
internal data class HermesAgentDto(
    val id: String = "",
    val title: String = "",
    val project: String = "",
    val status: String = "",
    val runtimeSeconds: Long? = null,
)

@Serializable
internal data class HermesAgentsDto(val available: Boolean = false, val agents: List<HermesAgentDto> = emptyList())

@Serializable
internal data class HermesReportDto(
    val id: String = "",
    val requestedAt: Long = 0,
    val completedAt: Long? = null,
    val text: String = "",
    val failed: Boolean = false,
)

@Serializable
internal data class HermesReportsDto(val available: Boolean = false, val reports: List<HermesReportDto> = emptyList())

@Serializable
internal data class VoiceRequestDto(
    val id: String = "",
    val at: Long = 0,
    val text: String = "",
    val reply: String? = null,
    val latencyMs: Long? = null,
    val failed: Boolean = false,
)

@Serializable
internal data class VoiceStatusDto(
    val relayUp: Boolean = false,
    val hermesUp: Boolean = false,
    val hermesVersion: String = "",
    val agentMode: Boolean = false,
    val requestsAvailable: Boolean = false,
    val requests: List<VoiceRequestDto> = emptyList(),
)

@Serializable
internal data class HermesSkillDto(
    val name: String = "",
    val description: String = "",
    val category: String? = null,
)

@Serializable
internal data class HermesSkillsDto(val available: Boolean = false, val skills: List<HermesSkillDto> = emptyList())

@Serializable
internal data class HermesToolsetDto(
    val name: String = "",
    val label: String = "",
    val description: String = "",
    val enabled: Boolean = false,
    val configured: Boolean = false,
    val tools: List<String> = emptyList(),
)

@Serializable
internal data class HermesToolsetsDto(val available: Boolean = false, val toolsets: List<HermesToolsetDto> = emptyList())

internal interface HermesPanelApi {
    @Idempotent
    @GET("v1/hermes/agents")
    suspend fun agents(): Outcome<HermesAgentsDto>

    @Idempotent
    @GET("v1/hermes/reports")
    suspend fun reports(): Outcome<HermesReportsDto>

    @Idempotent
    @GET("v1/hermes/voice")
    suspend fun voice(): Outcome<VoiceStatusDto>

    @Idempotent
    @GET("v1/hermes/skills")
    suspend fun skills(): Outcome<HermesSkillsDto>

    @Idempotent
    @GET("v1/hermes/toolsets")
    suspend fun toolsets(): Outcome<HermesToolsetsDto>
}

internal fun agentStatus(raw: String): HermesAgentStatus = when (raw) {
    "active", "starting", "pending", "stopping" -> HermesAgentStatus.RUNNING
    "idle" -> HermesAgentStatus.IDLE
    "error" -> HermesAgentStatus.FAILED
    else -> HermesAgentStatus.OTHER
}

internal fun HermesAgentsDto.toDomain(): HermesAgents = HermesAgents(
    available = available,
    agents = agents.map { HermesAgent(it.id, it.title, it.project, agentStatus(it.status), it.runtimeSeconds) },
)

internal fun HermesReportsDto.toDomain(): HermesReports = HermesReports(
    available = available,
    reports = reports.map { HermesReport(it.id, it.requestedAt, it.completedAt, it.text, it.failed) },
)

internal fun VoiceStatusDto.toDomain(): VoiceStatus = VoiceStatus(
    relayUp = relayUp,
    hermesUp = hermesUp,
    hermesVersion = hermesVersion.takeUnless { it.equals(UNKNOWN_VERSION, ignoreCase = true) }.orEmpty(),
    agentMode = agentMode,
    requestsAvailable = requestsAvailable,
    requests = requests.map { VoiceRequest(it.id, it.at, it.text, it.reply, it.latencyMs, it.failed) },
)

internal fun HermesSkillsDto.toDomain(): HermesSkills = HermesSkills(
    available = available,
    skills = skills.map { HermesSkill(it.name, it.description, it.category?.takeIf(String::isNotBlank)) },
)

internal fun HermesToolsetsDto.toDomain(): HermesToolsets = HermesToolsets(
    available = available,
    toolsets = toolsets.map {
        HermesToolset(it.name, it.label.ifBlank { it.name }, it.description, it.enabled, it.configured, it.tools)
    },
)

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpHermesPanelClient(private val http: HomeHttp) : HermesPanelClient {
    private val api: HermesPanelApi by lazy { http.rest(HermesPanelApi::class, Target.Control) }

    override suspend fun agents(): Outcome<HermesAgents> = api.agents().mapResult { it.toDomain() }

    override suspend fun reports(): Outcome<HermesReports> = api.reports().mapResult { it.toDomain() }

    override suspend fun voice(): Outcome<VoiceStatus> = api.voice().mapResult { it.toDomain() }

    override suspend fun skills(): Outcome<HermesSkills> = api.skills().mapResult { it.toDomain() }

    override suspend fun toolsets(): Outcome<HermesToolsets> = api.toolsets().mapResult { it.toDomain() }
}

private const val UNKNOWN_VERSION = "unknown"
