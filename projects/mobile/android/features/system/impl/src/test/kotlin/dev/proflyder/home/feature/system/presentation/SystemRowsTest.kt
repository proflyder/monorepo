package dev.proflyder.home.feature.system.presentation

import org.robolectric.RuntimeEnvironment
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.feature.system.SystemFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "en")
class SystemRowsTest {
    private val rows = SystemRows(RuntimeEnvironment.getApplication())

    @Test
    fun overviewPutsTheTrafficLightAndAlertsBeforeTheCards() {
        val overview = rows.overview(SystemFixtures.snapshot)
        assertEquals(
            listOf("status", "alert:0:KubePodCrashLooping", "alert:1:PinsStale", "card:host", "card:guests", "card:cluster", "card:devices"),
            overview.map { it.key },
        )
        val status = overview.first() as SystemRow.Status
        assertEquals(HealthLevel.CRITICAL, status.level)
        assertEquals("Problems detected", status.title)
        assertTrue(status.subtitle.startsWith("2 active alerts · updated "))
        val host = overview[3] as SystemRow.Card
        assertEquals("CPU 23% · RAM 63% · Disk 60%\n61 °C · up 17\u00A0d\u00A06\u00A0h", host.lines)
        val guests = overview[4] as SystemRow.Card
        assertEquals("Running 3 of 4\nStopped: vpn", guests.lines)
        assertEquals(HealthLevel.CRITICAL, guests.level)
        val cluster = overview[5] as SystemRow.Card
        assertEquals("Pods: 3 · problems: 1\ncurrency-bot-6c · CrashLoopBackOff", cluster.lines)
        val devices = overview[6] as SystemRow.Card
        assertEquals("Speaker v0.5.1020 · 32 ms\n1 phone", devices.lines)
    }

    @Test
    fun missingSourcesSayNoDataInsteadOfZeroes() {
        val overview = rows.overview(SystemFixtures.empty)
        val status = overview.first() as SystemRow.Status
        assertEquals("No data", status.title)
        assertTrue(status.subtitle.startsWith("Alerts unavailable"))
        val lines = overview.filterIsInstance<SystemRow.Card>().map { it.lines }
        assertTrue(lines[0].startsWith("No data"))
        assertTrue(lines[1].startsWith("No data"))
        assertTrue(lines[2].startsWith("No data"))
        assertEquals("No data from the speaker\nPhone registry unavailable", lines[3])
        assertTrue(rows.detail(SystemSection.CLUSTER, SystemFixtures.empty).single() is SystemRow.Note)
    }

    @Test
    fun clusterDetailGroupsPodsUnderNamespacesProblemsFirst() {
        val detail = rows.detail(SystemSection.CLUSTER, SystemFixtures.snapshot)
        assertEquals(
            listOf("ns:currency-bot", "pod:currency-bot/currency-bot-6c", "ns:home-assistant", "pod:home-assistant/scheduler-7d9", "pod:home-assistant/mcp-gateway-5f"),
            detail.map { it.key },
        )
        val crashing = detail[1] as SystemRow.Item
        assertEquals("CrashLoopBackOff · ready 0/1 · restarts 9 · 4m CPU · 21 MB · age 1\u00A0d\u00A00\u00A0h", crashing.subtitle)
    }

    @Test
    fun guestAndHostDetailsCarryMetricsAndLevels() {
        val guests = rows.detail(SystemSection.GUESTS, SystemFixtures.snapshot).map { it as SystemRow.Item }
        assertEquals("102 · vpn (CT)", guests.first().title)
        assertEquals("Stopped · starts on boot", guests.first().subtitle)
        assertEquals(HealthLevel.CRITICAL, guests.first().level)
        assertTrue(guests[1].subtitle.startsWith("Running · CPU 12% · RAM 2.1 GB / 4.0 GB (53%)"))
        val host = rows.detail(SystemSection.HOST, SystemFixtures.snapshot).map { it as SystemRow.Item }
        assertEquals(listOf("pve", "CPU", "Memory", "Disk", "Temperature", "Uptime"), host.map { it.title })
        assertEquals("10 GB / 16 GB (63%)", host[2].subtitle)
    }

    @Test
    fun levelsFollowTheThresholds() {
        assertEquals(HealthLevel.OK, ratioLevel(0.89))
        assertEquals(HealthLevel.WARNING, ratioLevel(0.9))
        assertEquals(HealthLevel.WARNING, tempLevel(75.0))
        assertEquals(HealthLevel.CRITICAL, tempLevel(85.0))
        assertEquals(SystemSection.CLUSTER, SystemSection.fromRoute("cluster"))
        assertEquals(SystemSection.HOST, SystemSection.fromRoute("bogus"))
    }
}
