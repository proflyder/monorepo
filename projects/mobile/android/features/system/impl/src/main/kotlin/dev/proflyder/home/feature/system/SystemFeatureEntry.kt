package dev.proflyder.home.feature.system

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.system.impl.R
import dev.proflyder.home.feature.system.presentation.SystemDetailFragment
import dev.proflyder.home.feature.system.presentation.SystemFragment
import dev.proflyder.home.feature.system.presentation.SystemSection
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object SystemOverviewDestination : Destination {
    override val featureId: String = "system"
    override val route: String = "system/overview"
}

data class SystemDetailDestination(val section: String) : Destination {
    override val featureId: String = "system"
    override val route: String = "system/detail/$section"

    override fun arguments(): Bundle = Bundle().apply { putString(ARG_SECTION, section) }

    companion object {
        const val ARG_SECTION = "system_section"
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

    override fun titleRes(destination: Destination): Int = when (destination) {
        is SystemDetailDestination -> SystemSection.fromRoute(destination.section).titleRes
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is SystemOverviewDestination || destination is SystemDetailDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is SystemDetailDestination -> SystemDetailFragment()
        else -> SystemFragment()
    }
}
