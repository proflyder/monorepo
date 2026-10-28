package dev.proflyder.home.feature.reminders

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.reminders.impl.R
import dev.proflyder.home.feature.reminders.presentation.ReminderEditorFragment
import dev.proflyder.home.feature.reminders.presentation.RemindersFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object ReminderListDestination : Destination {
    override val featureId: String = "reminders"
    override val route: String = "reminders/list"
}

data class ReminderEditorDestination(
    val reminderId: String? = null,
) : Destination {
    override val featureId: String = "reminders"
    override val route: String =
        if (reminderId == null) "reminders/editor/new" else "reminders/editor/$reminderId"

    override fun arguments(): Bundle = Bundle().apply {
        if (reminderId != null) putString(ARG_REMINDER_ID, reminderId)
    }

    companion object {
        const val ARG_REMINDER_ID = "reminder_id"
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class RemindersFeatureEntry : FeatureEntry {
    override val id: String = "reminders"
    override val order: Int = 2
    override val titleRes: Int = R.string.reminders_title
    override val iconRes: Int = R.drawable.reminders_ic_tab
    override val rootDestination: Destination = ReminderListDestination

    override fun titleRes(destination: Destination): Int = when (destination) {
        is ReminderEditorDestination ->
            if (destination.reminderId == null) R.string.reminders_editor_title_new else R.string.reminders_editor_title_edit
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is ReminderListDestination || destination is ReminderEditorDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is ReminderEditorDestination -> ReminderEditorFragment()
        else -> RemindersFragment()
    }
}
