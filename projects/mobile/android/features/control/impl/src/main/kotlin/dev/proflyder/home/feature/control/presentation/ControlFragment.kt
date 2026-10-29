package dev.proflyder.home.feature.control.presentation

import android.os.Bundle
import androidx.fragment.app.viewModels
import androidx.recyclerview.widget.GridLayoutManager
import com.google.android.material.snackbar.Snackbar
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFilterBinding

class ControlFragment : BaseFragment<FragmentControlBinding>(FragmentControlBinding.Companion::inflate) {
    private val viewModel: ControlViewModel by viewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private lateinit var adapter: ControlAdapter
    private lateinit var states: ListStateViews
    private var shownOptions: List<ControlFilterOption> = emptyList()
    private var syncingChips = false

    /** The single callback instance shared with the adapter. */
    private val adapterCallback = object : BaseAdapterCallback<ControlItem>(), ControlAdapterCallback {
        override fun onItemClick(item: ControlItem) {
            when (item) {
                is ControlItem.Scenario -> viewModel.run(item.entry)
                // A row tap flips the device; the switch itself reports its own target.
                is ControlItem.Device -> viewModel.setPower(item.entry, !item.entry.isOn())
                is ControlItem.Header -> Unit
            }
        }

        override fun onPowerChanged(entry: ControlEntry, on: Boolean) = viewModel.setPower(entry, on)
    }

    override fun onBindingCreated(binding: FragmentControlBinding, savedInstanceState: Bundle?) {
        adapter = ControlAdapter(adapterCallback)
        val spanCount = resources.getInteger(R.integer.control_columns)
        val deviceSpan = spanCount / resources.getInteger(R.integer.control_device_columns)
        binding.controlList.layoutManager = GridLayoutManager(requireContext(), spanCount).apply {
            spanSizeLookup = object : GridLayoutManager.SpanSizeLookup() {
                override fun getSpanSize(position: Int): Int = when (adapter.currentList.getOrNull(position)) {
                    is ControlItem.Scenario -> 1
                    is ControlItem.Device -> deviceSpan
                    else -> spanCount
                }
            }
        }
        binding.controlList.adapter = adapter

        states = ListStateViews(
            progress = binding.controlProgress,
            empty = binding.controlEmpty,
            error = binding.controlError,
            content = binding.controlContent,
            errorText = binding.controlErrorText,
        )
        binding.controlRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.controlRetry.setOnClickListener { viewModel.refresh() }
        binding.controlOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }

        binding.controlFilters.setOnCheckedStateChangeListener { _, checkedIds ->
            if (syncingChips) return@setOnCheckedStateChangeListener
            val id = checkedIds.firstOrNull() ?: return@setOnCheckedStateChangeListener
            shownOptions.firstOrNull { optionId(it) == id }?.let { viewModel.setFilter(it.filter) }
        }

        viewModel.state.observe { render(it) }
    }

    override fun onBindingDestroyed() {
        shownOptions = emptyList()
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun render(state: ControlUiState) {
        val hasItems = state.entries.isNotEmpty()
        val loadError = state.loadError
        when {
            !state.configured -> {
                states.showError(getString(R.string.control_not_configured))
                binding.controlRetry.setVisible(false)
                binding.controlOpenConnection.setVisible(true)
            }
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> {
                states.showError(loadError.userMessage(requireContext()))
                binding.controlRetry.setVisible(true)
                binding.controlOpenConnection.setVisible(false)
            }
            !hasItems -> states.showEmpty()
            else -> {
                states.showContent()
                renderFilters(state)
                adapter.submitList(ControlItems.build(state, state.filter, texts()))
            }
        }
        binding.controlRefresh.isRefreshing = state.refreshing && hasItems
        binding.controlStatesBanner.setVisible(hasItems && !state.statesOk)

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            viewModel.consumeActionError()
        }
        state.notice?.let { notice ->
            when (notice) {
                is ControlNotice.ScenarioQueued -> Snackbar.make(
                    binding.root,
                    getString(R.string.control_scenario_queued, notice.title),
                    Snackbar.LENGTH_SHORT,
                ).show()
            }
            viewModel.consumeNotice()
        }
    }

    private fun texts() = ControlTexts(
        otherDevices = getString(R.string.control_section_other),
        scenarios = getString(R.string.control_section_scenarios),
        unavailable = getString(R.string.control_section_unavailable),
        onSummary = { on, total -> getString(R.string.control_on_summary, on, total) },
    )

    /** Chips are stable per filter: ids are derived from it so re-checking never needs a rebuild. */
    private fun optionId(option: ControlFilterOption): Int = option.filter.hashCode().let { if (it == 0) 1 else it }

    private fun renderFilters(state: ControlUiState) {
        val options = ControlItems.options(state.entries)
        val group = binding.controlFilters
        syncingChips = true
        if (options != shownOptions) {
            group.removeAllViews()
            options.forEach { option ->
                val chip = ItemControlFilterBinding.inflate(layoutInflater, group, false).root
                chip.id = optionId(option)
                chip.text = labelOf(option)
                group.addView(chip)
            }
            shownOptions = options
        }
        options.firstOrNull { it.filter == state.filter }?.let { group.check(optionId(it)) }
        syncingChips = false
        binding.controlFiltersBar.setVisible(options.size > 1)
    }

    private fun labelOf(option: ControlFilterOption): String {
        val name = when (val filter = option.filter) {
            ControlFilter.All -> getString(R.string.control_filter_all)
            ControlFilter.Scenarios -> getString(R.string.control_filter_scenarios)
            ControlFilter.Lights -> getString(R.string.control_filter_lights)
            ControlFilter.Switches -> getString(R.string.control_filter_switches)
            is ControlFilter.Room -> filter.name
        }
        return "$name · ${option.count}"
    }

    private fun ControlEntry.isOn(): Boolean = power == dev.proflyder.home.core.model.PowerState.ON
}
