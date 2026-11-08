package dev.proflyder.home.feature.scenarios

import android.os.Bundle
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.presentation.ScenarioEditorFragment
import dev.proflyder.home.feature.scenarios.presentation.ScenariosFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object ScenarioListDestination : Destination {
    override val featureId: String = "scenarios"
    override val route: String = "scenarios/list"
}

data class ScenarioEditorDestination(
    val scenarioId: String? = null,
) : Destination {
    override val featureId: String = "scenarios"
    override val route: String =
        if (scenarioId == null) "scenarios/editor/new" else "scenarios/editor/$scenarioId"

    override fun arguments(): Bundle = Bundle().apply {
        if (scenarioId != null) putString(ARG_SCENARIO_ID, scenarioId)
    }

    companion object {
        const val ARG_SCENARIO_ID = "scenario_id"
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

    override fun titleRes(destination: Destination): Int = when (destination) {
        is ScenarioEditorDestination ->
            if (destination.scenarioId == null) {
                R.string.scenarios_editor_title_new
            } else {
                R.string.scenarios_editor_title_edit
            }
        else -> titleRes
    }

    override fun supports(destination: Destination): Boolean =
        destination is ScenarioListDestination || destination is ScenarioEditorDestination

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is ScenarioEditorDestination -> ScenarioEditorFragment()
        else -> ScenariosFragment()
    }
}
