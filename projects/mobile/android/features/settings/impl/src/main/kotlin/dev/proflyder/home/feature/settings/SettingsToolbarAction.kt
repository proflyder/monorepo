package dev.proflyder.home.feature.settings

import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.feature.settings.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarAction>())
@Inject
class SettingsToolbarAction : ToolbarAction {
    override val itemId: Int = R.id.settings_toolbar_action
    override val order: Int = 10
    override val titleRes: Int = R.string.settings_toolbar_action
    override val iconRes: Int = R.drawable.settings_ic_toolbar

    override fun perform(navigator: Navigator) {
        navigator.navigate(SettingsDestination)
    }
}
