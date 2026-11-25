package dev.proflyder.home.feature.scenarios.presentation

import android.os.Bundle
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.presentation.tabViewModels
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenariosBinding
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.fabContentInsets

internal class ScenariosFragment :
    ScreenFragment<FragmentScenariosBinding, ScenariosUiState, ScenariosIntent, ScenariosEvent>(
        FragmentScenariosBinding.Companion::inflate,
    ) {
    override val viewModel: ScenariosViewModel by tabViewModels()

    private val adapter = ScenarioAdapter(ScenarioAdapterIntents(::dispatch))

    override fun contentInsets(): ContentInsets = fabContentInsets()

    override fun onViewReady(binding: FragmentScenariosBinding, savedInstanceState: Bundle?) = with(binding) {
        scenariosList.adapter = adapter
        val states = ListStateViews(
            progress = scenariosProgress,
            empty = scenariosEmpty,
            error = scenariosError,
            content = scenariosList,
            errorText = scenariosErrorText,
        )
        scenariosRefresh.setupPullToRefresh(scenariosList) { dispatch(ScenariosIntent.Refresh(RefreshMode.HARD)) }
        scenariosRetry.setOnClickListener { dispatch(ScenariosIntent.Refresh(RefreshMode.HARD)) }
        scenariosOpenConnection.setOnClickListener { dispatch(ScenariosIntent.OpenConnection) }
        scenariosAdd.setOnClickListener { dispatch(ScenariosIntent.Add) }
        render(::scenariosLoadOf) { load -> renderLoad(states, load) }
        render(ScenariosUiState::rows, adapter::submitList)
    }

    override fun onBindingDestroyed() {
        binding.scenariosList.adapter = null
    }

    private fun renderLoad(states: ListStateViews, load: ScenariosLoad) {
        val loadError = load.error
        when {
            load.refreshing && !load.hasItems -> states.showLoading()
            loadError != null && !load.hasItems -> showLoadError(states, loadError)
            !load.hasItems -> states.showEmpty()
            else -> states.showContent()
        }
        binding.scenariosRefresh.isRefreshing = load.refreshing && load.hasItems
    }

    override fun onEvent(event: ScenariosEvent) {
        when (event) {
            is ScenariosEvent.Failure -> showFailure(event.failure)
            is ScenariosEvent.Queued -> notice(getString(R.string.scenarios_run_queued, event.label))
            is ScenariosEvent.Pinned -> notice(getString(pinnedText(event.pinned), event.label))
            is ScenariosEvent.OpenRoute -> navigate(event.route)
            is ScenariosEvent.ConfirmPin -> confirmPin(event)
        }
    }

    private fun showLoadError(states: ListStateViews, failure: RemoteFailure) {
        states.showError(failure.userMessage(requireContext()))
        binding.scenariosOpenConnection.setVisible(failure == RemoteFailure.NotConfigured)
    }

    private fun confirmPin(event: ScenariosEvent.ConfirmPin) = confirm(
        message = event.scenario.label,
        positive = if (event.pinned) R.string.scenarios_unpin else R.string.scenarios_pin,
        negative = android.R.string.cancel,
        destructive = false,
    ) { dispatch(ScenariosIntent.TogglePin(event.scenario)) }

    private fun pinnedText(pinned: Boolean): Int =
        if (pinned) R.string.scenarios_favorite_pinned else R.string.scenarios_favorite_unpinned

    private fun notice(text: String) = Notices.show(binding.root, text)
}

internal data class ScenariosLoad(val refreshing: Boolean, val error: RemoteFailure?, val hasItems: Boolean)

internal fun scenariosLoadOf(state: ScenariosUiState): ScenariosLoad =
    ScenariosLoad(state.refreshing, state.loadError, state.scenarios.isNotEmpty())
