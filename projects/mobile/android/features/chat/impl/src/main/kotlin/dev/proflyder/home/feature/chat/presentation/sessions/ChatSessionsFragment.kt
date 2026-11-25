package dev.proflyder.home.feature.chat.presentation.sessions

import android.os.Bundle
import android.view.Gravity
import android.view.View
import androidx.recyclerview.widget.ConcatAdapter
import dev.proflyder.home.core.designsystem.list.ListHeaderSpacer
import dev.proflyder.home.core.designsystem.list.addGroupedRows
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.bindText
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.fabContentInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.chat.ChatConversationDestination
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.ChatSessionsDestination
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatSessionsBinding

internal class ChatSessionsFragment :
    ScreenFragment<FragmentChatSessionsBinding, ChatSessionsUiState, ChatSessionsIntent, ChatSessionsEvent>(
        FragmentChatSessionsBinding::inflate,
    ) {
    override val viewModel: ChatSessionsViewModel by screenViewModel()

    private val adapter = ChatSessionsAdapter(Rows())

    override fun contentInsets(): ContentInsets = fabContentInsets()

    override fun onViewReady(binding: FragmentChatSessionsBinding, savedInstanceState: Bundle?) = with(binding) {
        chatSessionsList.adapter = ConcatAdapter(ListHeaderSpacer.leading(chatSessionsList, chatSessionsHeader), adapter)
        chatSessionsList.addGroupedRows(resources.getDimensionPixelSize(R.dimen.chat_session_divider_inset))
        chatSessionsRefresh.setupPullToRefresh(chatSessionsList) { dispatch(ChatSessionsIntent.Refresh) }
        chatSessionsRetry.setOnClickListener { dispatch(ChatSessionsIntent.Refresh) }
        chatSessionsNew.setOnClickListener { dispatch(ChatSessionsIntent.Create) }
        chatSessionsNew.setVisible(!viewModel.state.value.archived)
        val search = bindText(chatSessionsSearch) { ChatSessionsIntent.Search(it) }
        render(ChatSessionsUiState::query) { search.render(it) }
        render(SessionRows::build) { adapter.submitList(it) }
        render(ChatSessionsUiState::creating) { chatSessionsNew.isEnabled = !it }
        render(::listStatus, ::renderStatus)
    }

    override fun onStart() {
        super.onStart()
        ChatFocus.setListVisible(true)
    }

    override fun onStop() {
        ChatFocus.setListVisible(false)
        super.onStop()
    }

    override fun onEvent(event: ChatSessionsEvent) {
        when (event) {
            is ChatSessionsEvent.Open -> open(event.sessionId)
            is ChatSessionsEvent.Failure -> showFailure(event.failure)
        }
    }

    private fun open(sessionId: String) = navigate(Route.Screen(ChatConversationDestination(sessionId)))

    private fun renderStatus(status: ListStatus) = with(binding) {
        chatSessionsRefresh.isRefreshing = false
        chatSessionsProgress.setVisible(status.loading)
        chatSessionsError.setVisible(status.failure != null)
        chatSessionsErrorText.text = status.failure?.userMessage(requireContext())
        chatSessionsEmpty.setVisible(status.emptyText != null)
        status.emptyText?.let(chatSessionsEmpty::setText)
    }

    private inner class Rows : SessionRowListener {
        override fun onOpen(row: SessionRow.Item) = open(row.session.id)

        override fun onActions(row: SessionRow.Item, anchor: View) {
            SessionMenu(anchor, row.session) { dispatch(ChatSessionsIntent.Act(it)) }.show(Gravity.END)
        }

        override fun onArchive() = navigate(Route.Screen(ChatSessionsDestination.Archive))
    }
}

internal data class ListStatus(val loading: Boolean, val failure: RemoteFailure?, val emptyText: Int?)

internal fun listStatus(state: ChatSessionsUiState): ListStatus {
    val empty = state.loaded && state.visible.isEmpty() && (state.archived || state.archivedCount == 0 || state.query.isNotBlank())
    val text = when {
        !empty -> null
        state.query.isNotBlank() -> R.string.chat_session_nothing_found
        state.archived -> R.string.chat_session_archive_empty
        else -> R.string.chat_session_empty
    }
    val failure = state.failure.takeIf { state.sessions.isEmpty() }
    return ListStatus(loading = !state.loaded && failure == null, failure = failure, emptyText = text.takeIf { failure == null })
}
