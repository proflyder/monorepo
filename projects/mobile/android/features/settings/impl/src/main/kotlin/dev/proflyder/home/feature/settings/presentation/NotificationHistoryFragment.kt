package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import androidx.recyclerview.widget.ItemTouchHelper
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.ListItemIntent
import dev.proflyder.home.core.presentation.PagingBinder
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.UndoNotice
import dev.proflyder.home.core.presentation.render
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.SwipeActions
import dev.proflyder.home.core.ui.swipeAction
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentNotificationHistoryBinding
import java.time.LocalDate
import java.time.ZoneId

internal class NotificationHistoryFragment :
    ScreenFragment<FragmentNotificationHistoryBinding, NotificationHistoryUiState, HistoryIntent, HistoryEvent>(
        FragmentNotificationHistoryBinding.Companion::inflate,
    ) {
    override val viewModel: NotificationHistoryViewModel by screenViewModel()

    private val callback = object :
        NotificationHistoryAdapterCallback,
        AdapterCallback<HistoryRow> by IntentAdapterCallback(::dispatch, ::openIntent) {}

    private val adapter by lazy { NotificationHistoryAdapter(callback) }
    private val undo = UndoNotice(::requireView) { viewModel.paging.dispatch(ListItemIntent.Undo) }

    override fun onViewReady(binding: FragmentNotificationHistoryBinding, savedInstanceState: Bundle?) {
        binding.historyList.applyScrollBottomInset()
        PagingBinder(
            recyclerView = binding.historyList,
            states = ListStateViews(
                progress = binding.historyProgress,
                empty = binding.historyEmpty,
                error = binding.historyError,
                content = binding.historyList,
                errorText = binding.historyErrorText,
            ),
            content = adapter,
            lifecycleOwner = viewLifecycleOwner,
            handler = viewModel.paging,
            failureText = { it.userMessage(requireContext()) },
            dispatch = { viewModel.paging.dispatch(it) },
        )
        ItemTouchHelper(swipes()).attachToRecyclerView(binding.historyList)
        binding.historyList.addHistoryDecorations()
        binding.historyClear.setOnClickListener { confirmClear() }
        binding.historyReadAll.setOnClickListener { dispatch(HistoryIntent.ReadAll) }
        render({ rows(it) }, adapter::submitList)
        render(NotificationHistoryUiState::pages) { pages ->
            renderCount(pages.items.size)
            binding.historyReadAll.setVisible(pages.items.any { !it.read })
        }
        render(NotificationHistoryUiState::undo) { item -> undo.render(item?.let { getString(R.string.history_deleted) }) }
    }

    override fun onBindingDestroyed() {
        undo.clear()
        binding.historyList.adapter = null
    }

    private fun rows(state: NotificationHistoryUiState): List<HistoryRow> =
        HistoryRows.of(state.pages.items, LocalDate.now(), ZoneId.systemDefault())

    private fun renderCount(count: Int) = with(binding) {
        historyClear.setVisible(count > 0)
        historyCount.text = getString(R.string.history_count, count)
        historyCount.setVisible(count > 0)
    }

    private fun swipes(): SwipeActions<HistoryIntent> {
        val context = requireContext()
        return SwipeActions(
            context,
            left = swipeAction(context, ItemTouchHelper.LEFT, R.drawable.settings_ic_delete, com.google.android.material.R.attr.colorError, com.google.android.material.R.attr.colorOnError) { position ->
                entryAt(position)?.let { HistoryIntent.Remove(it.key) }
            },
            right = swipeAction(context, ItemTouchHelper.RIGHT, R.drawable.settings_ic_mark_unread, androidx.appcompat.R.attr.colorPrimary, com.google.android.material.R.attr.colorOnPrimary) { position ->
                entryAt(position)?.let { HistoryIntent.ToggleRead(it.key) }
            },
            itemAt = { position -> (adapter.currentList.getOrNull(position) as? HistoryRow.Item)?.entry },
            dispatch = ::dispatch,
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
        ) { dispatch(HistoryIntent.Clear) }
    }
}
