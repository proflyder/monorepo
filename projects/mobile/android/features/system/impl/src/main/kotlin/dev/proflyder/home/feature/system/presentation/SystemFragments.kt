package dev.proflyder.home.feature.system.presentation

import android.os.Bundle
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.system.SystemDetailDestination
import dev.proflyder.home.feature.system.SystemState
import dev.proflyder.home.feature.system.impl.databinding.FragmentSystemBinding

internal abstract class SystemScreenFragment :
    ScreenFragment<FragmentSystemBinding, SystemState, SystemIntent, SystemEvent>(FragmentSystemBinding.Companion::inflate) {
    override val viewModel: SystemViewModel by screenViewModel()

    private val adapter by lazy { SystemAdapter(IntentAdapterCallback(::dispatch, ::sectionIntent)) }

    abstract fun rows(snapshot: SystemSnapshot): List<SystemRow>

    override fun onScreenCreated(binding: FragmentSystemBinding, savedInstanceState: Bundle?) {
        binding.systemList.layoutManager = LinearLayoutManager(requireContext())
        binding.systemList.adapter = adapter
        binding.systemRefresh.setupPullToRefresh { dispatch(SystemIntent.Refresh) }
        binding.systemRetry.setOnClickListener { dispatch(SystemIntent.Refresh) }
    }

    private fun sectionIntent(row: SystemRow): SystemIntent? =
        (row as? SystemRow.Card)?.let { SystemIntent.OpenSection(it.section) }

    override fun onBindingDestroyed() {
        binding.systemList.adapter = null
    }

    override fun render(state: SystemState) {
        val snapshot = state.snapshot
        val error = state.error
        binding.systemRefresh.isRefreshing = state.loading && snapshot != null
        binding.systemProgress.setVisible(state.loading && snapshot == null && error == null)
        binding.systemError.setVisible(snapshot == null && error != null && !state.loading)
        binding.systemErrorText.text = error?.userMessage(requireContext())
        adapter.submitList(snapshot?.let { rows(it) }.orEmpty())
    }

    override fun onEvent(event: SystemEvent) {
        when (event) {
            is SystemEvent.RefreshFailed -> showFailure(event.failure)
            is SystemEvent.OpenRoute -> navigate(event.route)
        }
    }
}

internal class SystemFragment : SystemScreenFragment() {
    override fun rows(snapshot: SystemSnapshot): List<SystemRow> = SystemRows(requireContext()).overview(snapshot)
}

internal class SystemDetailFragment : SystemScreenFragment() {
    private val destination by destination(SystemDetailDestination)

    override fun rows(snapshot: SystemSnapshot): List<SystemRow> =
        SystemRows(requireContext()).detail(destination?.section ?: SystemSection.HOST, snapshot)
}
