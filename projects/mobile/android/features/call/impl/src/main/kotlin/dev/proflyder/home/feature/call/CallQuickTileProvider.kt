package dev.proflyder.home.feature.call

import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

@ContributesIntoSet(AppScope::class)
@Inject
class CallQuickTileProvider(
    private val launcher: CallLauncher,
) : QuickTileProvider {
    override val tile: ProvidedTile = ProvidedTile(
        key = "call",
        order = 0,
        labelRes = R.string.call_quick_tile_title,
        iconRes = R.drawable.call_ic_quick_tile,
    )

    override fun activate(): QuickTileAction {
        launcher.openCallScreen()
        return QuickTileAction.Handled
    }
}
