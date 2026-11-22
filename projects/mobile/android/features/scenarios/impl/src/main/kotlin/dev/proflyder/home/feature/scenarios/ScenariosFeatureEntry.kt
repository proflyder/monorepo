package dev.proflyder.home.feature.scenarios

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorFragment
import dev.proflyder.home.feature.scenarios.presentation.ScenariosFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object ScenarioListDestination : Destination {
    override val featureId: String = "scenarios"
    override val route: String = "scenarios/list"
}

internal data class ScenarioEditorDestination(
    val scenarioId: String? = null,
) : Destination {
    override val featureId: String = "scenarios"
    override val route: String = "scenarios/editor/${scenarioId ?: NEW}"

    companion object : RouteCodec<ScenarioEditorDestination> {
        private const val NEW = "new"

        override fun decode(path: RoutePath): ScenarioEditorDestination? =
            path.tail("scenarios", "editor")?.let { ScenarioEditorDestination(it.takeUnless { id -> id == NEW }) }
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ScenariosFeatureEntry : FeatureEntry {
    override val id: String = "scenarios"
    override val order: Int = 1
    override val titleRes: Int = R.string.scenarios_title
    override val iconRes: Int = R.drawable.scenarios_ic_tab
    override val rootDestination: Destination = ScenarioListDestination
    override val routes: List<RouteCodec<*>> =
        listOf(RouteCodec.fixed(ScenarioListDestination), ScenarioEditorDestination)

    override fun titleRes(destination: Destination): Int = when (destination) {
        is ScenarioEditorDestination ->
            if (destination.scenarioId == null) {
                R.string.scenarios_editor_title_new
            } else {
                R.string.scenarios_editor_title_edit
            }
        else -> titleRes
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is ScenarioEditorDestination -> ScenarioEditorFragment()
        else -> ScenariosFragment()
    }
}
