package dev.proflyder.home.feature.chat.presentation

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Bundle
import android.view.View
import androidx.appcompat.widget.PopupMenu
import androidx.core.content.getSystemService
import androidx.core.view.updatePadding
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatBinding

class ChatFragment : BaseFragment<FragmentChatBinding>(FragmentChatBinding.Companion::inflate) {
    internal val viewModel: ChatViewModel by activityViewModels {
        viewModelFactory(ChatViewModel::class.java)
    }

    private lateinit var adapter: ChatAdapter
    private lateinit var layoutManager: LinearLayoutManager
    private var lastCount = 0

    override fun onBindingCreated(binding: FragmentChatBinding, savedInstanceState: Bundle?) {
        adapter = ChatAdapter(Callbacks())
        layoutManager = LinearLayoutManager(requireContext()).apply { stackFromEnd = true }
        binding.chatList.layoutManager = layoutManager
        binding.chatList.adapter = adapter
        binding.chatList.addOnScrollListener(ScrollWatcher())
        ItemTouchHelper(SwipeToReply(::onSwipeReply)).attachToRecyclerView(binding.chatList)
        binding.chatPanel.pinAboveKeyboard(binding.chatList)
        binding.chatList.addOnLayoutChangeListener(StatesFollowList(binding.chatStates))
        binding.chatInput.doAfterTextChanged { binding.chatSend.isEnabled = !it.isNullOrBlank() }
        binding.chatSend.setOnClickListener { send() }
        binding.chatReplyCancel.setOnClickListener { viewModel.cancelReply() }
        binding.chatScrollDown.setOnClickListener { scrollToEnd(smooth = true) }
        binding.chatErrorRetry.setOnClickListener { viewModel.refresh() }
        viewModel.state.observe { render(it) }
        viewModel.errors.observe { showFailure(it) }
        repeatOnView { viewModel.runLive() }
    }

    internal fun render(state: ChatUiState) {
        val empty = state.items.isEmpty()
        binding.chatProgress.setVisible(!state.loaded && state.failure == null && empty)
        binding.chatError.setVisible(!state.loaded && state.failure != null && empty)
        binding.chatErrorText.text = state.failure?.userMessage(requireContext())
        binding.chatEmpty.setVisible(state.loaded && empty)
        renderReply(state)
        val follow = isAtEnd() || state.items.size > lastCount && state.items.lastOrNull().isMine()
        lastCount = state.items.size
        adapter.submitList(state.items) { if (follow) scrollToEnd(smooth = false) }
    }

    private fun renderReply(state: ChatUiState) {
        val quote = state.replyTo
        binding.chatReplyBar.setVisible(quote != null)
        quote ?: return
        binding.chatReplyAuthor.setText(chatAuthor(quote.role))
        binding.chatReplyText.text = ChatMarkdown(plainStyle()).render(quote.text).toString()
    }

    private fun plainStyle(): MarkdownStyle = MarkdownStyle(0, 0, 0, 0)

    private fun send() {
        val text = binding.chatInput.text?.toString().orEmpty()
        if (viewModel.send(text)) binding.chatInput.text = null
    }

    internal fun isAtEnd(): Boolean {
        val last = layoutManager.findLastVisibleItemPosition()
        return last == RecyclerView.NO_POSITION || last >= adapter.itemCount - 1
    }

    private fun scrollToEnd(smooth: Boolean) {
        val target = adapter.itemCount - 1
        if (target < 0) return
        if (smooth) binding.chatList.smoothScrollToPosition(target) else binding.chatList.scrollToPosition(target)
    }

    private fun onSwipeReply(position: Int) {
        adapter.bubbleAt(position)?.takeIf { it.quotable }?.let(viewModel::replyTo)
        adapter.notifyItemChanged(position)
        binding.chatInput.requestFocus()
    }

    internal fun showMenu(bubble: ChatItem.Bubble, anchor: View) {
        val popup = PopupMenu(requireContext(), anchor)
        popup.inflate(R.menu.chat_message_actions)
        popup.menu.findItem(R.id.chat_action_reply).isVisible = bubble.quotable
        popup.menu.findItem(R.id.chat_action_copy).isVisible = bubble.text.isNotEmpty()
        popup.menu.findItem(R.id.chat_action_retry).isVisible = bubble.retryable()
        popup.menu.findItem(R.id.chat_action_delete).isVisible = bubble.messageId == null
        popup.setOnMenuItemClickListener { item -> onMenuAction(bubble, item.itemId) }
        popup.show()
    }

    private fun onMenuAction(bubble: ChatItem.Bubble, itemId: Int): Boolean {
        when (itemId) {
            R.id.chat_action_reply -> viewModel.replyTo(bubble).also { binding.chatInput.requestFocus() }
            R.id.chat_action_copy -> copy(bubble.text)
            R.id.chat_action_retry -> viewModel.retry(bubble)
            R.id.chat_action_delete -> viewModel.discard(bubble)
            else -> Unit
        }
        return true
    }

    private fun copy(text: String) {
        val clipboard = requireContext().getSystemService<ClipboardManager>() ?: return
        clipboard.setPrimaryClip(ClipData.newPlainText(getString(R.string.chat_title), text))
        showMessage(R.string.chat_copied)
    }

    private fun ChatItem?.isMine(): Boolean = (this as? ChatItem.Bubble)?.mine == true

    private inner class Callbacks : ChatAdapterCallbacks() {
        override fun onBubbleMenu(bubble: ChatItem.Bubble, anchor: View) = showMenu(bubble, anchor)

        override fun onRetry(bubble: ChatItem.Bubble) = viewModel.retry(bubble)
    }

    private inner class ScrollWatcher : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
            binding.chatScrollDown.setVisible(!isAtEnd())
            val first = layoutManager.findFirstVisibleItemPosition()
            if (first in 0..OLDER_THRESHOLD && viewModel.state.value.hasMore) viewModel.loadOlder()
        }
    }

    private companion object {
        const val OLDER_THRESHOLD = 3
    }
}

internal class StatesFollowList(private val states: View) : View.OnLayoutChangeListener {
    override fun onLayoutChange(
        list: View,
        left: Int,
        top: Int,
        right: Int,
        bottom: Int,
        oldLeft: Int,
        oldTop: Int,
        oldRight: Int,
        oldBottom: Int,
    ) {
        if (states.paddingBottom != list.paddingBottom) states.post { states.updatePadding(bottom = list.paddingBottom) }
    }
}

internal class SwipeToReply(
    private val onSwipe: (Int) -> Unit,
) : ItemTouchHelper.SimpleCallback(0, ItemTouchHelper.LEFT) {
    override fun onMove(
        recyclerView: RecyclerView,
        viewHolder: RecyclerView.ViewHolder,
        target: RecyclerView.ViewHolder,
    ): Boolean = false

    override fun getSwipeDirs(recyclerView: RecyclerView, viewHolder: RecyclerView.ViewHolder): Int {
        val adapter = recyclerView.adapter as? ChatAdapter
        val bubble = adapter?.bubbleAt(viewHolder.bindingAdapterPosition)
        return if (bubble?.quotable == true) ItemTouchHelper.LEFT else 0
    }

    override fun getSwipeThreshold(viewHolder: RecyclerView.ViewHolder): Float = SWIPE_THRESHOLD

    override fun onSwiped(viewHolder: RecyclerView.ViewHolder, direction: Int) = onSwipe(viewHolder.bindingAdapterPosition)

    private companion object {
        const val SWIPE_THRESHOLD = 0.25f
    }
}
