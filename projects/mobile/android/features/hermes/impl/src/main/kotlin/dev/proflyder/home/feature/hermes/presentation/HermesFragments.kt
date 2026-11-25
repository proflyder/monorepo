package dev.proflyder.home.feature.hermes.presentation

import android.os.Bundle
import dev.proflyder.home.core.designsystem.list.addGroupedRows
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.hermes.HermesPanel
import dev.proflyder.home.feature.hermes.HermesReportDestination
import dev.proflyder.home.feature.hermes.HermesSectionDestination
import dev.proflyder.home.feature.hermes.HermesState
import dev.proflyder.home.feature.hermes.impl.databinding.FragmentHermesBinding

internal abstract class HermesScreenFragment :
    ScreenFragment<FragmentHermesBinding, HermesUiState, HermesIntent, HermesEvent>(FragmentHermesBinding.Companion::inflate) {
    override val viewModel: HermesViewModel by screenViewModel()

    private val adapter by lazy { HermesAdapter(IntentAdapterCallback(::dispatch, ::rowIntent)) }

    protected abstract val panel: HermesPanel?

    protected abstract fun rows(rows: HermesRows, state: HermesState): List<HermesRow>

    override fun onViewReady(binding: FragmentHermesBinding, savedInstanceState: Bundle?) {
        binding.hermesList.adapter = adapter
        binding.hermesList.addGroupedRows()
        binding.hermesRefresh.isEnabled = panel != null
        binding.hermesRefresh.setupPullToRefresh(binding.hermesList) { refresh() }
        binding.hermesRetry.setOnClickListener { refresh() }
        render({ rows(HermesRows(requireContext()), it.hermes) }) { adapter.submitList(it) }
        render(::status, ::renderStatus)
        refresh()
    }

    private fun refresh() {
        panel?.let { dispatch(HermesIntent.Refresh(it)) }
    }

    private fun rowIntent(row: HermesRow): HermesIntent? =
        (row as? HermesRow.Entry)?.target?.let(HermesIntent::Open)

    private fun status(state: HermesUiState): HermesStatus {
        val loaded = panel?.let(state.hermes::of)
        return HermesStatus(loaded?.loading == true, loaded?.failure, loaded == null || loaded.value != null)
    }

    private fun renderStatus(status: HermesStatus) = with(binding) {
        hermesRefresh.isRefreshing = status.loading && status.hasValue
        hermesProgress.setVisible(status.loading && !status.hasValue)
        hermesError.setVisible(!status.loading && !status.hasValue && status.failure != null)
        hermesErrorText.text = status.failure?.userMessage(requireContext())
    }

    override fun onEvent(event: HermesEvent) {
        when (event) {
            is HermesEvent.RefreshFailed -> showFailure(event.failure)
            is HermesEvent.OpenRoute -> navigate(event.route)
        }
    }

    override fun onBindingDestroyed() {
        binding.hermesList.adapter = null
    }
}

internal class HermesOverviewFragment : HermesScreenFragment() {
    override val panel: HermesPanel? = null

    override fun rows(rows: HermesRows, state: HermesState): List<HermesRow> = rows.overview()
}

internal class HermesSectionFragment : HermesScreenFragment() {
    private val destination by destination(HermesSectionDestination)

    private val section: HermesSection get() = destination?.section ?: HermesSection.AGENTS

    override val panel: HermesPanel get() = section.panel

    override fun rows(rows: HermesRows, state: HermesState): List<HermesRow> = rows.section(section, state)
}

internal class HermesReportFragment : HermesScreenFragment() {
    private val destination by destination(HermesReportDestination)

    override val panel: HermesPanel = HermesPanel.REPORTS

    override fun rows(rows: HermesRows, state: HermesState): List<HermesRow> =
        rows.report(state.reports.value?.reports?.firstOrNull { it.id == destination?.id })
}

internal data class HermesStatus(val loading: Boolean, val failure: RemoteFailure?, val hasValue: Boolean)
