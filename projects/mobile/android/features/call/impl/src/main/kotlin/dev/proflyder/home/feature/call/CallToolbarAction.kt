package dev.proflyder.home.feature.call

import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.feature.call.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarAction>())
@Inject
class CallToolbarAction(private val launcher: CallLauncher) : ToolbarAction {
    override val itemId: Int = R.id.call_toolbar_action
    override val order: Int = 0
    override val titleRes: Int = R.string.call_toolbar_action
    override val iconRes: Int = R.drawable.call_ic_toolbar

    override fun perform(navigator: Navigator) {
        launcher.openCallScreen()
    }
}
