package dev.proflyder.home.feature.scenarios.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.aspectPayload
import dev.proflyder.home.core.presentation.renderDimmed
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioBinding

internal data class ScenarioRow(
    val scenario: Scenario,
    val busy: Boolean,
)

internal interface ScenarioAdapterCallback : AdapterCallback<ScenarioRow> {
    fun onToggle(scenario: Scenario, enabled: Boolean)
    fun onRun(scenario: Scenario)
}

internal class ScenarioAdapterIntents(
    private val dispatch: (ScenariosIntent) -> Unit,
) : ScenarioAdapterCallback,
    AdapterCallback<ScenarioRow> by IntentAdapterCallback(
        dispatch,
        onClick = { row -> ScenariosIntent.Open(row.scenario) },
        onLongClick = { row -> ScenariosIntent.AskPin(row.scenario) },
    ) {
    override fun onToggle(scenario: Scenario, enabled: Boolean) = dispatch(ScenariosIntent.SetEnabled(scenario, enabled))

    override fun onRun(scenario: Scenario) = dispatch(ScenariosIntent.Run(scenario))
}

internal enum class ScenarioAspect {
    STATE,
    CONTENT,
}

internal val ScenarioDiff = KeyedDiff<ScenarioRow>(
    key = { it.scenario.id },
    payload = aspectPayload(
        mapOf(
            ScenarioAspect.STATE to { row: ScenarioRow -> row.scenario.enabled to row.busy },
            ScenarioAspect.CONTENT to { row: ScenarioRow -> row.scenario.copy(enabled = false) },
        ),
    ),
)

internal class ScenarioAdapter(
    callback: ScenarioAdapterCallback,
) : BaseListAdapter<ScenarioRow, ItemScenarioBinding, ScenarioAdapterCallback>(callback, ScenarioDiff) {

    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<ScenarioRow, ItemScenarioBinding, ScenarioAdapterCallback> =
        Holder(ItemScenarioBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemScenarioBinding,
        callback: ScenarioAdapterCallback,
    ) : BaseViewHolder<ScenarioRow, ItemScenarioBinding, ScenarioAdapterCallback>(binding, callback) {
        private val enabled = SwitchBinder(binding.scenarioEnabled) { checked -> callback.onToggle(item.scenario, checked) }

        init {
            forwardRowClicks()
            binding.scenarioRun.setOnClickListener { callback.onRun(item.scenario) }
        }

        override fun bind(item: ScenarioRow) {
            val scenario = item.scenario
            val context = binding.root.context
            binding.scenarioLabel.text = scenario.label
            binding.scenarioDescription.text = scenario.description
            binding.scenarioDescription.setVisible(scenario.description.isNotBlank())
            binding.scenarioTriggers.text = ScenarioFormat.triggerSummary(context, scenario)
            binding.scenarioNext.text = ScenarioFormat.nextDue(context, scenario)
            binding.scenarioUnsupported.setVisible(scenario.hasUnsupportedParts)
            renderState(item, animate = false)
        }

        override fun bindPayloads(item: ScenarioRow, payloads: List<Any>) {
            val aspects = PayloadSet.merge(payloads, ScenarioAspect::class.java)
            if (aspects == null || ScenarioAspect.CONTENT in aspects) bind(item) else renderState(item, animate = true)
        }

        private fun renderState(item: ScenarioRow, animate: Boolean) {
            binding.root.renderDimmed(!item.scenario.enabled, DISABLED_ALPHA, animate)
            binding.scenarioRun.isEnabled = !item.busy
            binding.scenarioEnabled.isEnabled = !item.busy
            enabled.render(item.scenario.enabled)
        }

        private companion object {
            const val DISABLED_ALPHA = 0.7f
        }
    }
}
