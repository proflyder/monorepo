package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.model.AnalyticsBackend
import dev.proflyder.home.core.model.AnalyticsEvent
import dev.proflyder.home.core.model.AppEnvironment
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.debug
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

@ContributesIntoSet(AppScope::class, binding = binding<AnalyticsBackend>())
@Inject
class LogAnalyticsBackend(private val logger: AppLogger) : AnalyticsBackend {
    override val environments: Set<AppEnvironment> = setOf(AppEnvironment.DEBUG)

    override fun trackScreen(screen: String) = logger.debug(TAG, "screen $screen")

    override fun track(event: AnalyticsEvent) = logger.debug(TAG, "event ${event.name} ${event.params}")

    private companion object {
        const val TAG = "Analytics"
    }
}
