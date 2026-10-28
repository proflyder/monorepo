package dev.proflyder.home.feature.connection

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.connection.impl.R
import dev.proflyder.home.feature.connection.presentation.ConnectionFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object ConnectionDestination : Destination {
    override val featureId: String = "connection"
    override val route: String = "connection/settings"
}

@ContributesIntoSet(AppScope::class)
@Inject
class ConnectionFeatureEntry : FeatureEntry {
    override val id: String = "connection"
    override val order: Int = 3
    override val titleRes: Int = R.string.connection_title
    override val iconRes: Int = R.drawable.connection_ic_tab
    override val rootDestination: Destination = ConnectionDestination

    override fun supports(destination: Destination): Boolean = destination is ConnectionDestination

    override fun createFragment(destination: Destination): Fragment = ConnectionFragment()
}
