package dev.proflyder.home.feature.hermes.presentation

import android.content.Context
import android.text.format.DateFormat
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.model.HermesAgent
import dev.proflyder.home.core.model.HermesAgentStatus
import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.core.model.HermesReport
import dev.proflyder.home.core.model.HermesReports
import dev.proflyder.home.core.model.HermesSkill
import dev.proflyder.home.core.model.HermesSkills
import dev.proflyder.home.core.model.HermesToolset
import dev.proflyder.home.core.model.HermesToolsets
import dev.proflyder.home.core.model.VoiceRequest
import dev.proflyder.home.core.model.VoiceStatus
import dev.proflyder.home.feature.hermes.HermesReportDestination
import dev.proflyder.home.feature.hermes.HermesSectionDestination
import dev.proflyder.home.feature.hermes.HermesState
import dev.proflyder.home.feature.hermes.HermesToolsetDestination
import dev.proflyder.home.feature.hermes.impl.R
import java.util.Date

internal class HermesRows(private val context: Context) {
    private val separator get() = context.getString(R.string.hermes_separator)

    fun overview(): List<HermesRow> = HermesSection.entries.map { section ->
        HermesRow.Entry(
            key = "section:${section.route}",
            title = context.getString(section.titleRes),
            subtitle = context.getString(section.subtitleRes),
            target = HermesSectionDestination(section),
        )
    }

    fun section(section: HermesSection, state: HermesState): List<HermesRow> = when (section) {
        HermesSection.AGENTS -> state.agents.value?.let(::agents).orEmpty()
        HermesSection.REPORTS -> state.reports.value?.let(::reports).orEmpty()
        HermesSection.VOICE -> state.voice.value?.let(::voice).orEmpty()
        HermesSection.TOOLSETS -> state.toolsets.value?.let(::toolsets).orEmpty()
        HermesSection.SKILLS -> state.skills.value?.let(::skills).orEmpty()
    }

    fun toolset(toolset: HermesToolset?): List<HermesRow> {
        if (toolset == null) return listOf(note("toolset:none", R.string.hermes_toolset_missing))
        val (status, tone) = toolsetState(toolset)
        return listOfNotNull(
            HermesRow.Entry("toolset:${toolset.name}", toolset.label, status, tone = tone),
            toolset.description.takeIf(String::isNotBlank)?.let { HermesRow.Note("toolset:description", it) },
            HermesRow.Header("toolset:tools", counted(R.string.hermes_toolset_tools, toolset.tools.size)),
        ) + toolRows(toolset)
    }

    fun report(report: HermesReport?): List<HermesRow> = when (report) {
        null -> listOf(note("report:none", R.string.hermes_report_missing))
        else -> listOf(
            HermesRow.Header("report:header", reportTime(report)),
            HermesRow.Body("report:body", report.text.ifEmpty { context.getString(R.string.hermes_report_empty) }),
        )
    }

    private fun agents(agents: HermesAgents): List<HermesRow> = when {
        !agents.available -> listOf(note("agents:none", R.string.hermes_agents_unavailable))
        agents.agents.isEmpty() -> listOf(note("agents:empty", R.string.hermes_agents_empty))
        else -> agents.agents.map(::agentRow)
    }

    private fun agentRow(agent: HermesAgent): HermesRow.Entry = HermesRow.Entry(
        key = "agent:${agent.id}",
        title = agent.title.ifEmpty { agent.id },
        subtitle = join(agent.project, agent.runtimeSeconds?.let(::runtime)),
        value = context.getString(agentStatusLabel(agent.status)),
        tone = agentTone(agent.status),
    )

    private fun reports(reports: HermesReports): List<HermesRow> = when {
        !reports.available -> listOf(note("reports:none", R.string.hermes_reports_unavailable))
        reports.reports.isEmpty() -> listOf(note("reports:empty", R.string.hermes_reports_empty))
        else -> reports.reports.map(::reportRow)
    }

    private fun reportRow(report: HermesReport): HermesRow.Entry = HermesRow.Entry(
        key = "report:${report.id}",
        title = dateTime(report.requestedAtUnix),
        subtitle = preview(report),
        tone = if (report.failed) Tone.ERROR else Tone.NEUTRAL,
        target = HermesReportDestination(report.id),
    )

    private fun preview(report: HermesReport): String = when {
        report.failed -> context.getString(R.string.hermes_report_failed)
        report.text.isEmpty() -> context.getString(R.string.hermes_report_empty)
        else -> report.text.lineSequence().first { it.isNotBlank() }.trim()
    }

    private fun voice(voice: VoiceStatus): List<HermesRow> = buildList {
        add(HermesRow.Header("voice:health", context.getString(R.string.hermes_voice_health)))
        add(stateRow("voice:proxy", R.string.hermes_voice_proxy, voice.relayUp, null))
        add(stateRow("voice:hermes", R.string.hermes_voice_hermes, voice.hermesUp, voice.hermesVersion.ifEmpty { null }))
        add(agentModeRow(voice.agentMode))
        add(HermesRow.Header("voice:recent", context.getString(R.string.hermes_voice_recent)))
        addAll(requests(voice))
    }

    private fun stateRow(key: String, title: Int, up: Boolean, version: String?): HermesRow.Entry = HermesRow.Entry(
        key = key,
        title = context.getString(title),
        subtitle = version?.let { context.getString(R.string.hermes_version, it) }.orEmpty(),
        value = context.getString(if (up) R.string.hermes_state_up else R.string.hermes_state_down),
        tone = if (up) Tone.SUCCESS else Tone.ERROR,
    )

    private fun agentModeRow(active: Boolean): HermesRow.Entry = HermesRow.Entry(
        key = "voice:mode",
        title = context.getString(R.string.hermes_voice_agent_mode),
        value = context.getString(if (active) R.string.hermes_mode_on else R.string.hermes_mode_off),
    )

    private fun requests(voice: VoiceStatus): List<HermesRow> = when {
        !voice.requestsAvailable -> listOf(note("voice:none", R.string.hermes_voice_unavailable))
        voice.requests.isEmpty() -> listOf(note("voice:empty", R.string.hermes_voice_empty))
        else -> voice.requests.map(::requestRow)
    }

    private fun requestRow(request: VoiceRequest): HermesRow.Entry = HermesRow.Entry(
        key = "request:${request.id}",
        title = request.text,
        subtitle = join(
            request.reply ?: context.getString(R.string.hermes_voice_no_reply),
            request.latencyMs?.let { context.getString(R.string.hermes_latency, it / MILLIS_IN_SECOND) },
        ),
        value = time(request.atUnix),
        tone = if (request.failed) Tone.ERROR else Tone.NEUTRAL,
    )

    private fun toolsets(toolsets: HermesToolsets): List<HermesRow> = when {
        !toolsets.available -> listOf(note("toolsets:none", R.string.hermes_toolsets_unavailable))
        toolsets.toolsets.isEmpty() -> listOf(note("toolsets:empty", R.string.hermes_toolsets_empty))
        else -> toolsets.toolsets.map(::toolsetRow)
    }

    private fun toolsetRow(toolset: HermesToolset): HermesRow.Entry {
        val (status, tone) = toolsetState(toolset)
        return HermesRow.Entry(
            key = "toolset:${toolset.name}",
            title = toolset.label,
            subtitle = status,
            value = context.resources.getQuantityString(R.plurals.hermes_tool_count, toolset.tools.size, toolset.tools.size),
            tone = tone,
            target = HermesToolsetDestination(toolset.name),
        )
    }

    private fun toolsetState(toolset: HermesToolset): Pair<String, Tone> = when {
        !toolset.enabled -> context.getString(R.string.hermes_toolset_disabled) to Tone.NEUTRAL
        !toolset.configured -> context.getString(R.string.hermes_toolset_unconfigured) to Tone.WARNING
        else -> context.getString(R.string.hermes_toolset_enabled) to Tone.SUCCESS
    }

    private fun toolRows(toolset: HermesToolset): List<HermesRow> = when {
        toolset.tools.isEmpty() -> listOf(note("tools:empty", R.string.hermes_tools_empty))
        else -> toolset.tools.map { HermesRow.Entry("tool:$it", it) }
    }

    private fun skills(skills: HermesSkills): List<HermesRow> = when {
        !skills.available -> listOf(note("skills:none", R.string.hermes_skills_unavailable))
        skills.skills.isEmpty() -> listOf(note("skills:empty", R.string.hermes_skills_empty))
        else -> skills.skills.groupBy(HermesSkill::category).flatMap { (category, group) -> skillGroup(category, group) }
    }

    private fun skillGroup(category: String?, skills: List<HermesSkill>): List<HermesRow> {
        val name = category ?: context.getString(R.string.hermes_skills_uncategorized)
        val header = HermesRow.Header("skills:$name", context.getString(R.string.hermes_counted, name, skills.size))
        return listOf(header) + skills.map { HermesRow.Entry("skill:${it.name}", it.name, it.description) }
    }

    private fun counted(title: Int, count: Int): String =
        context.getString(R.string.hermes_counted, context.getString(title), count)

    private fun reportTime(report: HermesReport): String = join(
        context.getString(R.string.hermes_report_requested, dateTime(report.requestedAtUnix)),
        report.completedAtUnix?.let { context.getString(R.string.hermes_report_completed, time(it)) },
    )

    private fun agentStatusLabel(status: HermesAgentStatus): Int = when (status) {
        HermesAgentStatus.RUNNING -> R.string.hermes_agent_running
        HermesAgentStatus.IDLE -> R.string.hermes_agent_idle
        HermesAgentStatus.FAILED -> R.string.hermes_agent_failed
        HermesAgentStatus.OTHER -> R.string.hermes_agent_other
    }

    private fun agentTone(status: HermesAgentStatus): Tone = when (status) {
        HermesAgentStatus.RUNNING -> Tone.SUCCESS
        HermesAgentStatus.FAILED -> Tone.ERROR
        HermesAgentStatus.IDLE, HermesAgentStatus.OTHER -> Tone.NEUTRAL
    }

    private fun runtime(seconds: Long): String = when {
        seconds >= SECONDS_IN_HOUR -> context.getString(
            R.string.hermes_runtime_hours,
            seconds / SECONDS_IN_HOUR,
            seconds % SECONDS_IN_HOUR / SECONDS_IN_MINUTE,
        )
        else -> context.getString(R.string.hermes_runtime_minutes, seconds / SECONDS_IN_MINUTE)
    }

    private fun note(key: String, text: Int): HermesRow.Note = HermesRow.Note(key, context.getString(text))

    private fun dateTime(unix: Long): String {
        val date = Date(unix * MILLIS)
        return DateFormat.getMediumDateFormat(context).format(date) + " " + DateFormat.getTimeFormat(context).format(date)
    }

    private fun time(unix: Long): String = DateFormat.getTimeFormat(context).format(Date(unix * MILLIS))

    private fun join(vararg parts: String?): String = parts.filterNot { it.isNullOrEmpty() }.joinToString(separator)

    private companion object {
        const val MILLIS = 1000L
        const val MILLIS_IN_SECOND = 1000.0
        const val SECONDS_IN_MINUTE = 60L
        const val SECONDS_IN_HOUR = 3600L
    }
}
