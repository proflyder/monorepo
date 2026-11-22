package dev.proflyder.home.core.testing

import dev.proflyder.home.core.model.AnalyticsBackend
import dev.proflyder.home.core.model.AnalyticsEvent
import dev.proflyder.home.core.model.AppEnvironment

class FakeAnalyticsTracker(
    override val environments: Set<AppEnvironment> = AppEnvironment.entries.toSet(),
) : AnalyticsBackend {
    val screens = mutableListOf<String>()
    val events = mutableListOf<AnalyticsEvent>()

    override fun trackScreen(screen: String) {
        screens += screen
    }

    override fun track(event: AnalyticsEvent) {
        events += event
    }
}
