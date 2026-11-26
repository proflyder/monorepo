package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.ItemTouchHelper
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.designsystem.list.ListHeaderSpacer
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ListItemIntent
import dev.proflyder.home.core.presentation.PagingBinder
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.UndoBinder
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.SwipeActions
import dev.proflyder.home.core.ui.SwipeStyle
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.swipeAction
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
        AdapterCallback<HistoryRow> by IntentAdapterCallback({ viewModel.dispatch(it) }, ::openIntent) {}

    private val adapter by lazy { NotificationHistoryAdapter(callback) }
    private val undo by lazy {
        UndoBinder<NotificationHistory>(::requireView, { getString(R.string.history_deleted) }, viewModel::dispatch)
    }

    override fun onViewReady(binding: FragmentNotificationHistoryBinding, savedInstanceState: Bundle?) {
        val binder = PagingBinder<NotificationHistory>(
            recyclerView = binding.historyList,
            states = ListStateViews(
                progress = binding.historyProgress,
                empty = binding.historyEmpty,
                error = binding.historyError,
                content = binding.historyList,
                errorText = binding.historyErrorText,
            ),
            content = ConcatAdapter(ListHeaderSpacer.leading(binding.historyList, binding.historyListHeader), adapter),
            failureText = { it.userMessage(requireContext()) },
            dispatch = viewModel::dispatch,
        )
        swipes().attachTo(binding.historyList)
        binding.historyList.addHistoryDecorations()
        binding.historyClear.setOnClickListener { confirmClear() }
        binding.historyReadAll.setOnClickListener { viewModel.dispatch(HistoryIntent.ReadAll) }
        render(NotificationHistoryUiState::rows, adapter::submitList)
        render(NotificationHistoryUiState::pages, binder::render)
        render(NotificationHistoryUiState::pendingRemoval, undo::render)
        render(NotificationHistoryUiState::pages) { pages -> renderCount(pages.items.size) }
        render(NotificationHistoryUiState::hasUnread) { binding.historyReadAll.setVisible(it) }
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

    private fun swipes(): SwipeActions {
        val context = requireContext()
        return SwipeActions(
            context,
            left = swipeAction(ItemTouchHelper.LEFT, SwipeStyle.Delete) { position ->
                entryAt(position)?.let { viewModel.dispatch(ListItemIntent.Remove(it.key)) }
            },
            right = swipeAction(ItemTouchHelper.RIGHT, SwipeStyle.ToggleRead) { position ->
                entryAt(position)?.let { viewModel.dispatch(HistoryIntent.ToggleRead(it.key)) }
            },
            itemAt = ::entryAt,
        )
    }

    private fun entryAt(position: Int): NotificationHistory? =
        (adapter.currentList.getOrNull(position) as? HistoryRow.Item)?.entry

    private fun openIntent(row: HistoryRow): HistoryIntent? = (row as? HistoryRow.Item)?.let { HistoryIntent.Open(it.entry) }

    override fun onEvent(event: HistoryEvent) {
        when (event) {
            HistoryEvent.Cleared -> showMessage(R.string.history_cleared)
            is HistoryEvent.OpenRoute -> navigate(event.route)
            is HistoryEvent.Failure -> showFailure(event.failure)
        }
    }

    private fun confirmClear() {
        confirm(
            message = R.string.history_clear_message,
            positive = R.string.history_clear,
            negative = android.R.string.cancel,
        ) { viewModel.dispatch(HistoryIntent.Clear) }
    }
}
