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
        assertEquals(listOf("Agents", "Reports", "Voice"), overview.map { it.title })
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
        assertEquals(Tone.ERROR, agents[2].tone)
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
        assertEquals("Дом в порядке.", reports[0].subtitle)
        assertEquals(HermesReportDestination("41"), reports[0].target)
        assertEquals("Hermes failed to finish the report", reports[1].subtitle)
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
        val request = entries.first { it.key == "request:9" }
        assertEquals("Будильник на 07:00 поставлен · 2.4 s", request.subtitle)
        val failed = entries.first { it.key == "request:7" }
        assertEquals("No reply", failed.subtitle)
        assertEquals(Tone.ERROR, failed.tone)
        assertNull(failed.target)
    }
}
