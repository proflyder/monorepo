package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding

internal class NotificationHistoryFragment :
    ScreenFragment<FragmentNotificationHistoryBinding, NotificationHistoryUiState, HistoryIntent, HistoryEvent>(
        FragmentNotificationHistoryBinding.Companion::inflate,
    ) {
    override val viewModel: NotificationHistoryViewModel by screenViewModel()

    private val callback = object :
        NotificationHistoryAdapterCallback,
        AdapterCallback<HistoryRow> by IntentAdapterCallback(::dispatch, ::openIntent) {}

    private val adapter by lazy { NotificationHistoryAdapter(callback) }

    override fun onViewReady(binding: FragmentNotificationHistoryBinding, savedInstanceState: Bundle?) {
        binding.historyList.applyScrollBottomInset()
        binding.historyList.adapter = adapter
        binding.historyList.addHistoryDecorations()
        binding.historyClear.setOnClickListener { confirmClear() }
        render(NotificationHistoryUiState::rows, adapter::submitList)
        render({ it.entries.size }, ::renderCount)
    }

    override fun onBindingDestroyed() {
        binding.historyList.adapter = null
    }

    private fun renderCount(count: Int) = with(binding) {
        historyClear.setVisible(count > 0)
        historyCount.text = getString(R.string.history_count, count)
        historyCount.setVisible(count > 0)
        historyEmpty.setVisible(count == 0)
    }

    override fun onEvent(event: HistoryEvent) {
        when (event) {
            HistoryEvent.Cleared -> showMessage(R.string.history_cleared)
            is HistoryEvent.OpenRoute -> navigate(event.route)
        }
    }

    private fun openIntent(row: HistoryRow): HistoryIntent? = (row as? HistoryRow.Item)?.let { HistoryIntent.Open(it.entry) }

    private fun confirmClear() {
        confirm(
            message = R.string.history_clear_message,
            positive = R.string.history_clear,
            negative = android.R.string.cancel,
        ) { dispatch(HistoryIntent.Clear) }
    }
}
