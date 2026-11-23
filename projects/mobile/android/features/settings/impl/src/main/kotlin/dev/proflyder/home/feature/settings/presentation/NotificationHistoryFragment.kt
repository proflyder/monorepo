package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.ItemTouchHelper
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.UndoNotice
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.PagingFooterAdapter
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.footer
import dev.proflyder.home.core.ui.loadMoreNearEnd
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
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
    private val footer = PagingFooterAdapter { dispatch(HistoryIntent.LoadMore) }
    private val undo = UndoNotice(::requireView) { dispatch(HistoryIntent.Undo) }

    override fun onViewReady(binding: FragmentNotificationHistoryBinding, savedInstanceState: Bundle?) {
        binding.historyList.applyScrollBottomInset()
        binding.historyList.adapter = ConcatAdapter(adapter, footer)
        binding.historyList.addHistoryDecorations()
        binding.historyList.loadMoreNearEnd { dispatch(HistoryIntent.LoadMore) }
        ItemTouchHelper(HistorySwipe(Swipes(), HistorySwipeBackground(requireContext()))).attachToRecyclerView(binding.historyList)
        val states = ListStateViews(
            progress = binding.historyProgress,
            empty = binding.historyEmpty,
            error = binding.historyError,
            content = binding.historyList,
            errorText = binding.historyErrorText,
        )
        binding.historyClear.setOnClickListener { confirmClear() }
        binding.historyReadAll.setOnClickListener { dispatch(HistoryIntent.ReadAll) }
        binding.historyRetry.setOnClickListener { dispatch(HistoryIntent.Refresh) }
        render(NotificationHistoryUiState::rows, adapter::submitList)
        render(NotificationHistoryUiState::paging) { state ->
            states.render(state) { failure -> failure.userMessage(requireContext()) }
            footer.render(state.footer())
            renderCount(state.items.size)
        }
        render(NotificationHistoryUiState::unread) { binding.historyReadAll.setVisible(it > 0) }
        render(NotificationHistoryUiState::undo) { deleted -> undo.render(deleted?.let { getString(R.string.history_deleted) }) }
    }

    override fun onBindingDestroyed() {
        undo.clear()
        binding.historyList.adapter = null
    }

    private fun renderCount(count: Int) = with(binding) {
        historyClear.setVisible(count > 0)
        historyCount.text = getString(R.string.history_count, count)
        historyCount.setVisible(count > 0)
    }

    override fun onEvent(event: HistoryEvent) {
        when (event) {
            HistoryEvent.Cleared -> showMessage(R.string.history_cleared)
            is HistoryEvent.OpenRoute -> navigate(event.route)
            is HistoryEvent.Failure -> showFailure(event.failure)
        }
    }

    private fun openIntent(row: HistoryRow): HistoryIntent? = (row as? HistoryRow.Item)?.let { HistoryIntent.Open(it.entry) }

    private inner class Swipes : HistorySwipeActions {
        override fun entryAt(position: Int) = (adapter.currentList.getOrNull(position) as? HistoryRow.Item)?.entry

        override fun onDelete(entry: NotificationHistory) = dispatch(HistoryIntent.Delete(entry))

        override fun onToggleRead(entry: NotificationHistory) = dispatch(HistoryIntent.ToggleRead(entry))
    }

    private fun confirmClear() {
        confirm(
            message = R.string.history_clear_message,
            positive = R.string.history_clear,
            negative = android.R.string.cancel,
        ) { dispatch(HistoryIntent.Clear) }
    }
}
