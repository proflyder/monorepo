package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import androidx.fragment.app.viewModels
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding

/**
 * The local log of what this phone received: newest first, an icon per type, a
 * relative time, and a tap that follows the message's own in-app deep link.
 *
 * The list is local-only; the server is not asked, so the screen also works
 * while the phone is offline. «Очистить» asks for confirmation first.
 */
class NotificationHistoryFragment :
    BaseFragment<FragmentNotificationHistoryBinding>(FragmentNotificationHistoryBinding.Companion::inflate) {
    private val viewModel: NotificationHistoryViewModel by viewModels {
        viewModelFactory(NotificationHistoryViewModel::class.java)
    }

    private lateinit var adapter: NotificationHistoryAdapter

    private val callback = object : BaseAdapterCallback<NotificationHistoryEntry>(),
        NotificationHistoryAdapterCallback {
        override fun onItemClick(item: NotificationHistoryEntry) = openLink(item)
    }

    override fun onBindingCreated(binding: FragmentNotificationHistoryBinding, savedInstanceState: Bundle?) {
        adapter = NotificationHistoryAdapter(callback)
        binding.historyList.layoutManager = LinearLayoutManager(requireContext())
        binding.historyList.adapter = adapter
        binding.historyClear.setOnClickListener { confirmClear() }
        viewModel.state.observe { render(it) }
    }

    override fun onBindingDestroyed() {
        binding.historyList.adapter = null
    }

    private fun render(state: NotificationHistoryUiState) {
        adapter.submitList(state.entries)
        binding.historyClear.setVisible(!state.isEmpty)
        binding.historyCount.text = getString(R.string.history_count, state.entries.size)
        binding.historyCount.setVisible(!state.isEmpty)
        binding.historyEmpty.setVisible(state.isEmpty)
        state.notice?.let { notice ->
            when (notice) {
                HistoryNotice.CLEARED -> showMessage(R.string.history_cleared)
            }
            viewModel.consumeNotice()
        }
    }

    /** A row without a deep link has nothing to open; the type icon still shows what it was. */
    internal fun openLink(entry: NotificationHistoryEntry) {
        val route = entry.deepLink ?: return
        requireNavigator().openRoute(route)
    }

    private fun confirmClear() {
        confirm(
            message = R.string.history_clear_message,
            positive = R.string.history_clear,
            negative = android.R.string.cancel,
        ) { viewModel.clear() }
    }
}
