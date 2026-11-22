package dev.proflyder.home.core.model

enum class AppEnvironment { DEBUG, RELEASE }

data class AnalyticsEvent(
    val name: String,
    val params: Map<String, String> = emptyMap(),
)

interface AnalyticsTracker {
    fun trackScreen(screen: String)

    fun track(event: AnalyticsEvent)
}

interface AnalyticsBackend : AnalyticsTracker {
    val environments: Set<AppEnvironment>
}

object NoOpAnalyticsTracker : AnalyticsTracker {
    override fun trackScreen(screen: String) = Unit

    override fun track(event: AnalyticsEvent) = Unit
}

class CompositeAnalyticsTracker(
    backends: Collection<AnalyticsBackend>,
    environment: AppEnvironment,
) : AnalyticsTracker {
    private val active = backends.filter { environment in it.environments }

    override fun trackScreen(screen: String) = active.forEach { it.trackScreen(screen) }

    override fun track(event: AnalyticsEvent) = active.forEach { it.track(event) }
}
