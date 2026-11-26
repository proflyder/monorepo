package dev.proflyder.home.feature.hermes

import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.feature.hermes.presentation.HermesRow
import dev.proflyder.home.feature.hermes.presentation.HermesRows
import dev.proflyder.home.feature.hermes.presentation.HermesSection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "en")
class HermesRowsTest {
    private val rows = HermesRows(RuntimeEnvironment.getApplication())

    @Test
    fun theOverviewLinksEverySection() {
        val overview = rows.overview().map { it as HermesRow.Entry }
        assertEquals(listOf("Agents", "Reports", "Voice", "Toolsets", "Skills"), overview.map { it.title })
        assertEquals(
            HermesSection.entries.map(::HermesSectionDestination),
            overview.map { it.target },
        )
    }

    @Test
    fun agentsShowStatusAndRuntimeWithoutTargets() {
        val agents = rows.section(HermesSection.AGENTS, HermesFixtures.state).map { it as HermesRow.Entry }
        assertEquals("Station Lab · 12 min", agents[0].subtitle)
        assertEquals("Running", agents[0].value)
        assertEquals(Tone.SUCCESS, agents[0].tone)
        assertEquals("Waiting for you", agents[1].value)
        assertEquals(Tone.WARNING, agents[1].tone)
        assertEquals("Done", agents[2].value)
        assertEquals(Tone.ERROR, agents[3].tone)
        assertTrue(agents.all { it.target == null })
    }

    @Test
    fun anUnavailableBridgeIsANote() {
        val state = HermesFixtures.state.copy(agents = Loaded(value = HermesAgents(available = false, agents = emptyList())))
        val note = rows.section(HermesSection.AGENTS, state).single() as HermesRow.Note
        assertEquals("The agent server is unavailable.", note.text)
    }

    @Test
    fun reportsPreviewTheFirstLineAndOpenTheReport() {
        val reports = rows.section(HermesSection.REPORTS, HermesFixtures.state).map { it as HermesRow.Entry }
        assertEquals("App chat · Дом в порядке.", reports[0].subtitle)
        assertEquals(HermesReportDestination("41"), reports[0].target)
        assertEquals("Telegram · Hermes failed to finish the report", reports[1].subtitle)
        assertEquals(Tone.ERROR, reports[1].tone)
        val detail = rows.report(HermesFixtures.reports.reports.first())
        assertEquals(HermesFixtures.reports.reports.first().text, (detail.last() as HermesRow.Body).text)
        assertTrue(rows.report(null).single() is HermesRow.Note)
    }

    @Test
    fun voiceShowsHealthThenRequestsWithLatency() {
        val voice = rows.section(HermesSection.VOICE, HermesFixtures.state)
        assertEquals(listOf("Health", "Recent requests"), voice.filterIsInstance<HermesRow.Header>().map { it.title })
        val entries = voice.filterIsInstance<HermesRow.Entry>()
        assertEquals("Version 0.12.0", entries[1].subtitle)
        assertEquals(Tone.NEUTRAL, entries[1].tone)
        assertEquals(Tone.NEUTRAL, entries[1].valueTone)
        val request = entries.first { it.key == "request:9" }
        assertEquals("Будильник на 07:00 поставлен · 2.4 s", request.subtitle)
        val failed = entries.first { it.key == "request:7" }
        assertEquals("No reply", failed.subtitle)
        assertEquals(Tone.ERROR, failed.tone)
        assertNull(failed.target)
        val mode = entries.first { it.key == "voice:mode" }
        assertEquals("Station voice to a bb thread", mode.title)
        assertEquals("When on, speaker phrases go to the bb thread created by voice instead of Hermes", mode.subtitle)
        assertEquals("Off", mode.value)
    }

    @Test
    fun aMissingVersionHidesTheSubtitleAndADownServiceIsRed() {
        val state = HermesFixtures.state.copy(voice = Loaded(value = HermesFixtures.voice.copy(hermesUp = false, hermesVersion = "")))
        val hermes = rows.section(HermesSection.VOICE, state).filterIsInstance<HermesRow.Entry>()[1]
        assertEquals("", hermes.subtitle)
        assertEquals(Tone.ERROR, hermes.valueTone)
    }

    @Test
    fun toolsetsShowStateAndToolCountAndOpenTheirTools() {
        val toolsets = rows.section(HermesSection.TOOLSETS, HermesFixtures.state).map { it as HermesRow.Entry }
        assertEquals(listOf("Web search", "Image generation", "Browser"), toolsets.map { it.title })
        assertEquals("Enabled", toolsets[0].subtitle)
        assertEquals("2 tools", toolsets[0].value)
        assertEquals(Tone.SUCCESS, toolsets[0].tone)
        assertEquals(Tone.WARNING, toolsets[1].tone)
        assertEquals("Disabled", toolsets[2].subtitle)
        assertEquals(HermesToolsetDestination("web"), toolsets[0].target)
    }

    @Test
    fun aToolsetListsItsToolsAndAMissingOneIsExplained() {
        val detail = rows.toolset(HermesFixtures.toolsets.named("browser"))
        assertEquals(
            listOf("toolset:browser", "toolset:description", "toolset:tools", "tool:browser_click", "tool:browser_open", "tool:browser_type"),
            detail.map { it.key },
        )
        assertEquals("Tools · 3", (detail[2] as HermesRow.Header).title)
        assertEquals(listOf("toolset:none"), rows.toolset(null).map { it.key })
    }

    @Test
    fun skillsAreGroupedByCategory() {
        val skills = rows.section(HermesSection.SKILLS, HermesFixtures.state)
        assertEquals(
            listOf(
                "skills:No category", "skill:notes",
                "skills:home", "skill:alarm-helper", "skill:report",
                "skills:research", "skill:arxiv",
            ),
            skills.map { it.key },
        )
        assertEquals("home · 2", (skills[2] as HermesRow.Header).title)
    }

    @Test
    fun anUnavailableCatalogIsANote() {
        val state = HermesState(
            toolsets = Loaded(value = HermesFixtures.toolsets.copy(available = false)),
            skills = Loaded(value = HermesFixtures.skills.copy(available = false, skills = emptyList())),
        )
        assertEquals(listOf("toolsets:none"), rows.section(HermesSection.TOOLSETS, state).map { it.key })
        assertEquals(listOf("skills:none"), rows.section(HermesSection.SKILLS, state).map { it.key })
    }
}
