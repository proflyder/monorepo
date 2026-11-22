package dev.proflyder.home.feature.scenarios.presentation

import android.os.Bundle
import androidx.recyclerview.widget.GridLayoutManager
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.presentation.RefreshMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.injectedActivityViewModels
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.FragmentScenariosBinding

internal class ScenariosFragment :
    ScreenFragment<FragmentScenariosBinding, ScenariosUiState, ScenariosIntent, ScenariosEvent>(
        FragmentScenariosBinding.Companion::inflate,
    ) {
    override val viewModel: ScenariosViewModel by injectedActivityViewModels()

    private lateinit var adapter: ScenarioAdapter
    private lateinit var states: ListStateViews

    override fun onScreenCreated(binding: FragmentScenariosBinding, savedInstanceState: Bundle?) = with(binding) {
        scenariosList.applyScrollBottomInset(scenariosList.fabClearance())
        scenariosAdd.liftAboveBottomInsets()
        adapter = ScenarioAdapter(ScenarioAdapterIntents(::dispatch))
        scenariosList.layoutManager = GridLayoutManager(requireContext(), resources.getInteger(R.integer.scenarios_columns))
        scenariosList.adapter = adapter
        states = ListStateViews(
            progress = scenariosProgress,
            empty = scenariosEmpty,
            error = scenariosError,
            content = scenariosList,
            errorText = scenariosErrorText,
        )
        scenariosRefresh.setupPullToRefresh { dispatch(ScenariosIntent.Refresh(RefreshMode.HARD)) }
        scenariosRetry.setOnClickListener { dispatch(ScenariosIntent.Refresh(RefreshMode.HARD)) }
        scenariosOpenConnection.setOnClickListener { dispatch(ScenariosIntent.OpenConnection) }
        scenariosAdd.setOnClickListener { dispatch(ScenariosIntent.Add) }
    }

    override fun render(state: ScenariosUiState) {
        val hasItems = state.scenarios.isNotEmpty()
        val loadError = state.loadError
        when {
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> showLoadError(loadError)
            !hasItems -> states.showEmpty()
            else -> states.showContent().also { adapter.submitList(state.scenarios.map { ScenarioRow(it, state.busy) }) }
        }
        binding.scenariosRefresh.isRefreshing = state.refreshing && hasItems
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

    private fun showLoadError(failure: RemoteFailure) {
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
