package dev.proflyder.home.feature.time

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.time.impl.R
import dev.proflyder.home.feature.time.presentation.TimeFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.Inject

internal data class TimeDestination(val section: TimeSection = TimeSection.ALARMS) : Destination {
    override val featureId: String = FEATURE_ID
    override val route: String = "$FEATURE_ID/${section.featureId}"

    companion object : RouteCodec<TimeDestination> {
        const val FEATURE_ID = "time"

        override fun decode(path: RoutePath): TimeDestination? =
            TimeSection.fromFeatureId(path.tail(FEATURE_ID))?.let(::TimeDestination)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class TimeFeatureEntry : FeatureEntry {
    override val id: String = TimeDestination.FEATURE_ID
    override val order: Int = 2
    override val module: AppModule = AppModule.ALARMS_TIMERS
    override val titleRes: Int = R.string.time_title
    override val iconRes: Int = R.drawable.time_ic_tab
    override val rootDestination: Destination = TimeDestination()
    override val routes: List<RouteCodec<*>> = listOf(TimeDestination)

    override fun createFragment(destination: Destination): Fragment = TimeFragment()

    override fun hostedDestination(featureId: String): Destination? =
        TimeSection.fromFeatureId(featureId)?.let(::TimeDestination)
}

@ContributesTo(AppScope::class)
interface TimeGraph {
    val featureRegistry: FeatureRegistry
}
