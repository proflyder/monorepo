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

class ControlFragment : BaseFragment<FragmentControlBinding>(FragmentControlBinding.Companion::inflate) {
    private val viewModel: ControlViewModel by viewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private lateinit var adapter: ControlAdapter
    private lateinit var states: ListStateViews

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
            content = binding.controlList,
            errorText = binding.controlErrorText,
        )
        binding.controlRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.controlRetry.setOnClickListener { viewModel.refresh() }
        binding.controlOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }

        viewModel.state.observe { render(it) }
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
                adapter.submitList(
                    ControlItems.build(
                        state,
                        scenariosTitle = getString(R.string.control_section_scenarios),
                        devicesTitle = getString(R.string.control_section_devices),
                    ),
                )
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

    private fun ControlEntry.isOn(): Boolean = power == dev.proflyder.home.core.model.PowerState.ON
}
