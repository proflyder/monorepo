package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider
import dev.proflyder.home.feature.vpn.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

@ContributesIntoSet(AppScope::class)
@Inject
class VpnQuickTileProvider : QuickTileProvider {
    override val tile: ProvidedTile = ProvidedTile(
        key = "vpn",
        order = 1,
        labelRes = R.string.vpn_quick_tile_title,
        iconRes = R.drawable.vpn_ic_quick_tile,
        module = AppModule.VPN,
    )

    override fun activate(): QuickTileAction = QuickTileAction.OpenFeature(VPN_FEATURE_ID)

    private companion object {
        const val VPN_FEATURE_ID = "vpn"
    }
}
