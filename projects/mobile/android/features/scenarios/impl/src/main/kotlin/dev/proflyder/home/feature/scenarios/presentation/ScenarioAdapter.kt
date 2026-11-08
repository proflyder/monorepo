package dev.proflyder.home.feature.scenarios.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.impl.databinding.ItemScenarioBinding

/** One list row: the scenario plus the in-flight mutation flag. */
internal data class ScenarioRow(
    val scenario: Scenario,
    val busy: Boolean,
)

/** Row click opens the editor; the switch and Run button are per-card actions. */
internal interface ScenarioAdapterCallback : AdapterCallback<ScenarioRow> {
    fun onToggle(scenario: Scenario, enabled: Boolean)
    fun onRun(scenario: Scenario)
}

private object ScenarioDiff : DiffUtil.ItemCallback<ScenarioRow>() {
    override fun areItemsTheSame(oldItem: ScenarioRow, newItem: ScenarioRow): Boolean =
        oldItem.scenario.id == newItem.scenario.id

    override fun areContentsTheSame(oldItem: ScenarioRow, newItem: ScenarioRow): Boolean = oldItem == newItem
}

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
        /** Programmatic `isChecked` must not be reported as a user toggle. */
        private var bindingSwitch = false

        init {
            forwardRowClicks()
            binding.scenarioEnabled.setOnCheckedChangeListener { _, checked ->
                if (!bindingSwitch) callback.onToggle(item.scenario, checked)
            }
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
            binding.root.alpha = if (scenario.enabled) 1f else 0.7f
            binding.scenarioRun.isEnabled = !item.busy
            binding.scenarioEnabled.isEnabled = !item.busy
            bindingSwitch = true
            binding.scenarioEnabled.isChecked = scenario.enabled
            bindingSwitch = false
            binding.scenarioUnsupported.setVisible(scenario.hasUnsupportedParts)
        }
    }
}
