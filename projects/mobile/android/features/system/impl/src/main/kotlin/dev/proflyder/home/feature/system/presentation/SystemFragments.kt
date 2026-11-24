package dev.proflyder.home.feature.system.presentation

import android.os.Bundle
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.system.SystemDetailDestination
import dev.proflyder.home.feature.system.impl.databinding.FragmentSystemBinding

internal abstract class SystemScreenFragment :
    ScreenFragment<FragmentSystemBinding, SystemUiState, SystemIntent, SystemEvent>(FragmentSystemBinding.Companion::inflate) {
    override val viewModel: SystemViewModel by screenViewModel()

    private val adapter by lazy { SystemAdapter(IntentAdapterCallback(::dispatch, ::rowIntent)) }

    abstract fun rows(snapshot: SystemSnapshot, administrator: Boolean): List<SystemRow>

    override fun onViewReady(binding: FragmentSystemBinding, savedInstanceState: Bundle?) {
        binding.systemList.adapter = adapter
        binding.systemRefresh.setupPullToRefresh(binding.systemList) { dispatch(SystemIntent.Refresh) }
        binding.systemRetry.setOnClickListener { dispatch(SystemIntent.Refresh) }
        render(::rowsOf) { rows -> adapter.submitList(rows) }
        render({ SystemStatus(it.loading, it.error, it.snapshot.snapshot != null) }, ::renderStatus)
    }

    private fun rowsOf(state: SystemUiState): List<SystemRow> =
        state.snapshot.snapshot?.let { rows(it, state.administrator) }.orEmpty()

    private fun rowIntent(row: SystemRow): SystemIntent? = when (row) {
        is SystemRow.Card -> SystemIntent.OpenSection(row.section)
        is SystemRow.Link -> SystemIntent.OpenAccess
        else -> null
    }

    override fun onBindingDestroyed() {
        binding.systemList.adapter = null
    }

    private fun renderStatus(status: SystemStatus) {
        val error = status.error
        val hasSnapshot = status.hasSnapshot
        binding.systemRefresh.isRefreshing = status.loading && hasSnapshot
        binding.systemProgress.setVisible(status.loading && !hasSnapshot && error == null)
        binding.systemError.setVisible(!hasSnapshot && error != null && !status.loading)
        binding.systemErrorText.text = error?.userMessage(requireContext())
    }

    override fun onEvent(event: SystemEvent) {
        when (event) {
            is SystemEvent.RefreshFailed -> showFailure(event.failure)
            is SystemEvent.OpenRoute -> navigate(event.route)
        }
    }
}

internal class SystemFragment : SystemScreenFragment() {
    override fun rows(snapshot: SystemSnapshot, administrator: Boolean): List<SystemRow> =
        SystemRows(requireContext()).overview(snapshot, administrator)
}

internal class SystemDetailFragment : SystemScreenFragment() {
    private val destination by destination(SystemDetailDestination)

    override fun rows(snapshot: SystemSnapshot, administrator: Boolean): List<SystemRow> =
        SystemRows(requireContext()).detail(destination?.section ?: SystemSection.HOST, snapshot)
}

internal data class SystemStatus(val loading: Boolean, val error: RemoteFailure?, val hasSnapshot: Boolean)
