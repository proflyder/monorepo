package dev.proflyder.home.feature.alarms

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.presentation.AlarmEditorFragment
import dev.proflyder.home.feature.alarms.presentation.AlarmsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object AlarmListDestination : Destination {
    override val featureId: String = "alarms"
    override val route: String = "alarms/list"
}

internal data class AlarmEditorDestination(
    val alarmId: String? = null,
) : Destination {
    override val featureId: String = "alarms"
    override val route: String = "alarms/editor/${alarmId ?: NEW}"

    companion object : RouteCodec<AlarmEditorDestination> {
        private const val NEW = "new"

        override fun decode(path: RoutePath): AlarmEditorDestination? =
            path.tail("alarms", "editor")?.let { AlarmEditorDestination(it.takeUnless { id -> id == NEW }) }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class AlarmsFeatureEntry : FeatureEntry {
    override val id: String = "alarms"
    override val order: Int = 2
    override val module: AppModule = AppModule.ALARMS_TIMERS
    override val isTab: Boolean = false
    override val titleRes: Int = R.string.alarms_title
    override val iconRes: Int = R.drawable.alarms_ic_tab
    override val rootDestination: Destination = AlarmListDestination
    override val routes: List<RouteCodec<*>> = listOf(RouteCodec.fixed(AlarmListDestination), AlarmEditorDestination)

    override fun titleRes(destination: Destination): Int = when (destination) {
        is AlarmEditorDestination ->
            if (destination.alarmId == null) R.string.alarms_editor_title_new else R.string.alarms_editor_title_edit
        else -> titleRes
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is AlarmEditorDestination -> AlarmEditorFragment()
        else -> AlarmsFragment()
    }
}
