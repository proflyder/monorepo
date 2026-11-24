package dev.proflyder.home.feature.reminders

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.presentation.ReminderEditorFragment
import dev.proflyder.home.feature.reminders.presentation.RemindersFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object ReminderListDestination : Destination {
    override val featureId: String = "reminders"
    override val route: String = "reminders/list"
}

internal data class ReminderEditorDestination(
    val reminderId: String? = null,
) : Destination {
    override val featureId: String = "reminders"
    override val route: String = "reminders/editor/${reminderId ?: NEW}"

    companion object : RouteCodec<ReminderEditorDestination> {
        private const val NEW = "new"

        override fun decode(path: RoutePath): ReminderEditorDestination? =
            path.tail("reminders", "editor")?.let { ReminderEditorDestination(it.takeUnless { id -> id == NEW }) }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class RemindersFeatureEntry : FeatureEntry {
    override val id: String = "reminders"
    override val order: Int = 4
    override val module: AppModule = AppModule.ALARMS_TIMERS
    override val isTab: Boolean = false
    override val titleRes: Int = R.string.reminders_title
    override val iconRes: Int = R.drawable.reminders_ic_tab
    override val rootDestination: Destination = ReminderListDestination
    override val routes: List<RouteCodec<*>> =
        listOf(RouteCodec.fixed(ReminderListDestination), ReminderEditorDestination)

    override fun titleRes(destination: Destination): Int = when (destination) {
        is ReminderEditorDestination ->
            if (destination.reminderId == null) R.string.reminders_editor_title_new else R.string.reminders_editor_title_edit
        else -> titleRes
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is ReminderEditorDestination -> ReminderEditorFragment()
        else -> RemindersFragment()
    }
}
