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
