package dev.proflyder.home.feature.alarms

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.presentation.AlarmEditorFragment
import dev.proflyder.home.feature.alarms.presentation.AlarmsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object AlarmListDestination : Destination {
    override val featureId: String = "alarms"
    override val route: String = "alarms/list"
}

data class AlarmEditorDestination(
    val alarmId: String? = null,
) : Destination {
    override val featureId: String = "alarms"
    override val route: String = if (alarmId == null) "alarms/editor/new" else "alarms/editor/$alarmId"

    override fun arguments(): Bundle = Bundle().apply {
        if (alarmId != null) putString(ARG_ALARM_ID, alarmId)
    }

    companion object {
        const val ARG_ALARM_ID = "alarm_id"
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class AlarmsFeatureEntry : FeatureEntry {
    override val id: String = "alarms"
    override val order: Int = 1
    override val titleRes: Int = R.string.alarms_title
    override val iconRes: Int = R.drawable.alarms_ic_tab
    override val rootDestination: Destination = AlarmListDestination

    override fun titleRes(destination: Destination): Int = when (destination) {
        is AlarmEditorDestination ->
            if (destination.alarmId == null) R.string.alarms_editor_title_new else R.string.alarms_editor_title_edit
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is AlarmListDestination || destination is AlarmEditorDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is AlarmEditorDestination -> AlarmEditorFragment()
        else -> AlarmsFragment()
    }
}
