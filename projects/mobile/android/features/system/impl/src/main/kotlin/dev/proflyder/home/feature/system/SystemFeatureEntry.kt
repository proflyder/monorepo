package dev.proflyder.home.feature.system

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.system.impl.R
import dev.proflyder.home.feature.system.presentation.SystemDetailFragment
import dev.proflyder.home.feature.system.presentation.SystemFragment
import dev.proflyder.home.feature.system.presentation.SystemSection
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object SystemOverviewDestination : Destination {
    override val featureId: String = "system"
    override val route: String = "system/overview"
}

internal data class SystemDetailDestination(val section: SystemSection) : Destination {
    override val featureId: String = "system"
    override val route: String = "system/detail/${section.route}"

    companion object : RouteCodec<SystemDetailDestination> {
        override fun decode(path: RoutePath): SystemDetailDestination? =
            SystemSection.fromRouteOrNull(path.tail("system", "detail"))?.let(::SystemDetailDestination)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class SystemFeatureEntry : FeatureEntry {
    override val id: String = "system"
    override val order: Int = 3
    override val titleRes: Int = R.string.system_title
    override val iconRes: Int = R.drawable.system_ic_tab
    override val rootDestination: Destination = SystemOverviewDestination
    override val routes: List<RouteCodec<*>> =
        listOf(RouteCodec.fixed(SystemOverviewDestination), SystemDetailDestination)

    override fun titleRes(destination: Destination): Int = when (destination) {
        is SystemDetailDestination -> destination.section.titleRes
        else -> titleRes
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is SystemDetailDestination -> SystemDetailFragment()
        else -> SystemFragment()
    }
}
