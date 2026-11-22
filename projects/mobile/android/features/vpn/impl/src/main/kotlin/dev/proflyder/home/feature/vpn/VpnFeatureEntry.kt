package dev.proflyder.home.feature.vpn

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.presentation.VpnFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object VpnDestination : Destination {
    override val featureId: String = "vpn"
    override val route: String = "vpn/main"
}

@ContributesIntoSet(AppScope::class)
@Inject
class VpnFeatureEntry : FeatureEntry {
    override val id: String = "vpn"
    override val order: Int = 8
    override val isTab: Boolean = false
    override val hidesBottomBar: Boolean = true
    override val titleRes: Int = R.string.vpn_title
    override val iconRes: Int = R.drawable.vpn_ic_shield
    override val rootDestination = VpnDestination

    override fun supports(destination: Destination): Boolean = destination is VpnDestination

    override fun createFragment(destination: Destination): Fragment = VpnFragment()
}
