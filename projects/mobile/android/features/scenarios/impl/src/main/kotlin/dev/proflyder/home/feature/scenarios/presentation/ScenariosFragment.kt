package dev.proflyder.home.feature.scenarios.presentation

import android.os.Bundle
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.scenarios.ScenarioEditorDestination
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenariosBinding

class ScenariosFragment : BaseFragment<FragmentScenariosBinding>(FragmentScenariosBinding.Companion::inflate) {
    internal val viewModel: ScenariosViewModel by activityViewModels {
        viewModelFactory(ScenariosViewModel::class.java)
    }

    private lateinit var adapter: ScenarioAdapter
    private lateinit var states: ListStateViews

    private val adapterCallback = object : BaseAdapterCallback<ScenarioRow>(), ScenarioAdapterCallback {
        override fun onItemClick(item: ScenarioRow) = onScenarioClick(item.scenario)
        override fun onToggle(scenario: Scenario, enabled: Boolean) = viewModel.setEnabled(scenario, enabled)
        override fun onRun(scenario: Scenario) = viewModel.run(scenario)
    }

    override fun onBindingCreated(binding: FragmentScenariosBinding, savedInstanceState: Bundle?) {
        adapter = ScenarioAdapter(adapterCallback)
        binding.scenariosList.layoutManager =
            GridLayoutManager(requireContext(), resources.getInteger(R.integer.scenarios_columns))
        binding.scenariosList.adapter = adapter
        states = ListStateViews(
            progress = binding.scenariosProgress,
            empty = binding.scenariosEmpty,
            error = binding.scenariosError,
            content = binding.scenariosList,
            errorText = binding.scenariosErrorText,
        )
        binding.scenariosRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.scenariosRetry.setOnClickListener { viewModel.refresh() }
        binding.scenariosOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }
        binding.scenariosAdd.setOnClickListener { requireNavigator().navigate(ScenarioEditorDestination()) }

        viewModel.state.observe { render(it) }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh(silent = true)
    }

    private fun render(state: ScenariosUiState) {
        val hasItems = state.scenarios.isNotEmpty()
        val loadError = state.loadError
        when {
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> {
                states.showError(loadError.userMessage(requireContext()))
                binding.scenariosOpenConnection.setVisible(loadError == RemoteFailure.NotConfigured)
            }
            !hasItems -> states.showEmpty()
            else -> {
                states.showContent()
                adapter.submitList(state.scenarios.map { ScenarioRow(it, state.busy) })
            }
        }
        binding.scenariosRefresh.isRefreshing = state.refreshing && hasItems

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            adapter.notifyItemRangeChanged(0, adapter.itemCount)
            viewModel.consumeActionError()
        }
        state.queuedLabel?.let { label ->
            Notices.show(binding.root, getString(R.string.scenarios_run_queued, label))
            viewModel.consumeQueued()
        }
    }

    internal fun onScenarioClick(scenario: Scenario) {
        requireNavigator().navigate(ScenarioEditorDestination(scenario.id))
    }
}
