package dev.proflyder.home.feature.hermes

import android.net.Uri
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.hermes.impl.R
import dev.proflyder.home.feature.hermes.presentation.HermesOverviewFragment
import dev.proflyder.home.feature.hermes.presentation.HermesReportFragment
import dev.proflyder.home.feature.hermes.presentation.HermesSection
import dev.proflyder.home.feature.hermes.presentation.HermesSectionFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object HermesOverviewDestination : Destination {
    override val featureId: String = HermesRoutes.FEATURE_ID
    override val route: String = HermesRoutes.OVERVIEW
}

internal data class HermesSectionDestination(val section: HermesSection) : Destination {
    override val featureId: String = HermesRoutes.FEATURE_ID
    override val route: String = "${HermesRoutes.SECTION_PREFIX}/${section.route}"

    companion object : RouteCodec<HermesSectionDestination> {
        override fun decode(path: RoutePath): HermesSectionDestination? =
            HermesSection.fromRouteOrNull(path.tail(HermesRoutes.FEATURE_ID, "section"))?.let(::HermesSectionDestination)
    }
}

internal data class HermesReportDestination(val id: String) : Destination {
    override val featureId: String = HermesRoutes.FEATURE_ID
    override val route: String = "${HermesRoutes.REPORT_PREFIX}/${Uri.encode(id)}"

    companion object : RouteCodec<HermesReportDestination> {
        override fun decode(path: RoutePath): HermesReportDestination? =
            path.tail(HermesRoutes.FEATURE_ID, "report")
                ?.let(Uri::decode)
                ?.takeIf(String::isNotBlank)
                ?.let(::HermesReportDestination)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class HermesFeatureEntry : FeatureEntry {
    override val id: String = HermesRoutes.FEATURE_ID
    override val order: Int = 22
    override val isTab: Boolean = false
    override val module: AppModule = AppModule.CHAT
    override val ownerOnly: Boolean = true
    override val titleRes: Int = R.string.hermes_title
    override val iconRes: Int = R.drawable.hermes_ic_agent
    override val rootDestination: Destination = HermesOverviewDestination
    override val routes: List<RouteCodec<*>> = listOf(
        RouteCodec.fixed(HermesOverviewDestination),
        HermesSectionDestination,
        HermesReportDestination,
    )

    override fun titleRes(destination: Destination): Int = when (destination) {
        is HermesSectionDestination -> destination.section.titleRes
        is HermesReportDestination -> R.string.hermes_report_title
        else -> titleRes
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is HermesSectionDestination -> HermesSectionFragment()
        is HermesReportDestination -> HermesReportFragment()
        else -> HermesOverviewFragment()
    }
}
