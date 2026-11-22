package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.model.AnalyticsEvent
import dev.proflyder.home.core.model.AppEnvironment
import dev.proflyder.home.core.model.CompositeAnalyticsTracker
import dev.proflyder.home.core.model.LogLevel
import dev.proflyder.home.core.testing.FakeAnalyticsTracker
import dev.proflyder.home.core.testing.FakeAppLogger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ObservabilityTest {
    @Test
    fun compositeFansOutOnlyToBackendsOfTheCurrentEnvironment() {
        val everywhere = FakeAnalyticsTracker()
        val debugOnly = FakeAnalyticsTracker(setOf(AppEnvironment.DEBUG))
        val tracker = CompositeAnalyticsTracker(listOf(everywhere, debugOnly), AppEnvironment.RELEASE)

        tracker.trackScreen("Alarms")
        tracker.track(AnalyticsEvent("alarm_saved"))

        assertEquals(listOf("Alarms"), everywhere.screens)
        assertEquals(listOf(AnalyticsEvent("alarm_saved")), everywhere.events)
        assertTrue(debugOnly.screens.isEmpty())
    }

    @Test
    fun logBackendWritesScreensThroughTheLogger() {
        val logger = FakeAppLogger()

        LogAnalyticsBackend(logger).trackScreen("Settings")

        assertEquals(LogLevel.DEBUG, logger.entries.single().level)
        assertTrue(logger.entries.single().message.contains("Settings"))
    }

    @Test
    fun releaseLoggerDropsMessagesBelowWarn() {
        ShadowLog.clear()
        val logger = ReleaseAppLogger()

        logger.log(LogLevel.DEBUG, "Tag", "hidden")
        logger.log(LogLevel.WARN, "Tag", "shown")

        assertEquals(listOf("shown"), ShadowLog.getLogsForTag("Tag").map { it.msg })
    }

    @Test
    fun debugLoggerKeepsDebugMessages() {
        ShadowLog.clear()

        DebugAppLogger().log(LogLevel.DEBUG, "Debug", "visible")

        assertEquals(listOf("visible"), ShadowLog.getLogsForTag("Debug").map { it.msg })
    }
}
