package dev.proflyder.home.feature.system.presentation

import android.os.Bundle
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.system.SystemDetailDestination
import dev.proflyder.home.feature.system.impl.databinding.FragmentSystemBinding
import kotlinx.coroutines.delay

internal const val AUTO_REFRESH_MILLIS = 30_000L

abstract class SystemScreenFragment : BaseFragment<FragmentSystemBinding>(FragmentSystemBinding.Companion::inflate) {
    internal val viewModel: SystemViewModel by activityViewModels {
        viewModelFactory(SystemViewModel::class.java)
    }

    private lateinit var adapter: SystemAdapter

    internal abstract fun rows(snapshot: SystemSnapshot): List<SystemRow>

    internal open fun onSection(section: SystemSection) = Unit

    override fun onBindingCreated(binding: FragmentSystemBinding, savedInstanceState: Bundle?) {
        adapter = SystemAdapter(
            object : SystemAdapterCallbacks() {
                override fun onSectionClick(section: SystemSection) = onSection(section)
            },
        )
        binding.systemList.layoutManager = LinearLayoutManager(requireContext())
        binding.systemList.adapter = adapter
        binding.systemRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.systemRetry.setOnClickListener { viewModel.refresh() }
        viewModel.state.observe { render(it) }
        repeatOnView { autoRefresh() }
    }

    private suspend fun autoRefresh() {
        while (true) {
            viewModel.refresh()
            delay(AUTO_REFRESH_MILLIS)
        }
    }

    internal fun render(state: SystemUiState) {
        val snapshot = state.snapshot
        val error = state.error
        binding.systemRefresh.isRefreshing = state.loading && snapshot != null
        binding.systemProgress.setVisible(state.loading && snapshot == null && error == null)
        binding.systemError.setVisible(snapshot == null && error != null && !state.loading)
        binding.systemErrorText.text = error?.userMessage(requireContext())
        adapter.submitList(snapshot?.let { rows(it) }.orEmpty())
        when {
            snapshot != null && error != null && !state.loading -> showFailure(error)
            else -> Unit
        }
    }
}

class SystemFragment : SystemScreenFragment() {
    override fun rows(snapshot: SystemSnapshot): List<SystemRow> = SystemRows(requireContext()).overview(snapshot)

    override fun onSection(section: SystemSection) =
        requireNavigator().navigate(SystemDetailDestination(section.route))
}

class SystemDetailFragment : SystemScreenFragment() {
    private val section: SystemSection
        get() = SystemSection.fromRoute(requireArguments().getString(SystemDetailDestination.ARG_SECTION))

    override fun rows(snapshot: SystemSnapshot): List<SystemRow> =
        SystemRows(requireContext()).detail(section, snapshot)
}
