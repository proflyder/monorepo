package dev.proflyder.home.core.model

enum class HermesAgentStatus {
    RUNNING,
    IDLE,
    FAILED,
    OTHER,
}

data class HermesAgent(
    val id: String,
    val title: String,
    val project: String,
    val status: HermesAgentStatus,
    val runtimeSeconds: Long?,
)

data class HermesAgents(val available: Boolean, val agents: List<HermesAgent>)

data class HermesReport(
    val id: String,
    val requestedAtUnix: Long,
    val completedAtUnix: Long?,
    val text: String,
    val failed: Boolean,
)

data class HermesReports(val available: Boolean, val reports: List<HermesReport>)

data class VoiceRequest(
    val id: String,
    val atUnix: Long,
    val text: String,
    val reply: String?,
    val latencyMs: Long?,
    val failed: Boolean,
)

data class VoiceStatus(
    val relayUp: Boolean,
    val hermesUp: Boolean,
    val hermesVersion: String,
    val agentMode: Boolean,
    val requestsAvailable: Boolean,
    val requests: List<VoiceRequest>,
)

data class HermesSkill(
    val name: String,
    val description: String,
    val category: String?,
)

data class HermesSkills(
    val available: Boolean,
    val skills: List<HermesSkill>,
)

data class HermesToolset(
    val name: String,
    val label: String,
    val description: String,
    val enabled: Boolean,
    val configured: Boolean,
    val tools: List<String>,
)

data class HermesToolsets(
    val available: Boolean,
    val toolsets: List<HermesToolset>,
) {
    fun named(name: String): HermesToolset? = toolsets.firstOrNull { it.name == name }
}
