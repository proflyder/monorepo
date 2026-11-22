package dev.proflyder.home.feature.chat.presentation

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.text.format.DateUtils
import android.view.View
import android.widget.ImageView
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.content.getSystemService
import androidx.core.net.toUri
import androidx.core.view.updatePadding
import androidx.core.widget.doAfterTextChanged
import androidx.fragment.app.activityViewModels
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.chip.Chip
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.Components
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.data.AttachmentSource
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatBinding
import kotlinx.coroutines.launch

internal class ChatFragment : ScreenFragment<FragmentChatBinding, ChatUiState, ChatIntent, ChatEvent>(
    FragmentChatBinding.Companion::inflate,
) {
    override val viewModel: ChatViewModel by activityViewModels { viewModelFactory(ChatViewModel::class.java) }

    private lateinit var adapter: ChatAdapter
    private lateinit var layoutManager: LinearLayoutManager
    private var lastCount = 0

    private val pickImage = registerForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri?.let { dispatch(ChatIntent.AttachImage(AttachmentSource(it.toString()))) }
    }

    private val pickFile = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { dispatch(ChatIntent.AttachFile(AttachmentSource(it.toString()))) }
    }

    private val askMicrophone = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (!granted) showError(R.string.chat_record_permission)
    }

    override fun onScreenCreated(binding: FragmentChatBinding, savedInstanceState: Bundle?) = with(binding) {
        adapter = ChatAdapter(Callbacks())
        layoutManager = LinearLayoutManager(requireContext()).apply { stackFromEnd = true }
        chatList.layoutManager = layoutManager
        chatList.adapter = adapter
        chatList.addOnScrollListener(ScrollWatcher())
        ItemTouchHelper(SwipeToReply(::onSwipeReply)).attachToRecyclerView(chatList)
        chatPanel.pinAboveKeyboard(chatList)
        chatList.addOnLayoutChangeListener(StatesFollowList(chatStates))
        chatInput.doAfterTextChanged { dispatch(ChatIntent.InputChanged(it?.toString().orEmpty())) }
        chatSend.setOnClickListener { dispatch(ChatIntent.Send) }
        chatMic.setOnTouchListener(RecordTouch(Recording(), resources.getDimension(R.dimen.chat_swipe_reply_distance)))
        chatAttach.setOnClickListener { HomePopupMenu(it, R.menu.chat_attach, ::onAttachChoice).show() }
        chatReplyCancel.setOnClickListener { dispatch(ChatIntent.CancelReply) }
        chatScrollDown.setOnClickListener { scrollToEnd(smooth = true) }
        chatErrorRetry.setOnClickListener { dispatch(ChatIntent.Refresh) }
    }

    override fun render(state: ChatUiState) {
        val empty = state.items.isEmpty()
        binding.chatProgress.setVisible(!state.loaded && state.failure == null && empty)
        binding.chatError.setVisible(!state.loaded && state.failure != null && empty)
        binding.chatErrorText.text = state.failure?.userMessage(requireContext())
        binding.chatEmpty.setVisible(state.loaded && empty)
        renderComposer(state)
        renderReply(state)
        renderPending(state)
        renderRecording(state.recording)
        val follow = isAtEnd() || state.items.size > lastCount && state.items.lastOrNull().isMine()
        lastCount = state.items.size
        adapter.submitList(state.items) { afterSubmit(follow) }
    }

    override fun onEvent(event: ChatEvent) {
        when (event) {
            is ChatEvent.Failure -> showFailure(event.failure)
            is ChatEvent.Rejected -> showError(chatErrorText(event.key))
        }
    }

    override fun onStart() {
        super.onStart()
        ChatFocus.setVisible(true)
    }

    override fun onStop() {
        ChatFocus.setVisible(false)
        super.onStop()
    }

    private fun afterSubmit(follow: Boolean) {
        val focused = ChatFocus.peek()?.let(::positionOf)
        when {
            focused != null -> layoutManager.scrollToPositionWithOffset(focused, 0).also { ChatFocus.take() }
            follow -> scrollToEnd(smooth = false)
            else -> Unit
        }
    }

    private fun positionOf(messageId: String): Int? = adapter.currentList.indexOfFirst {
        (it as? ChatItem.Bubble)?.messageId == messageId || (it as? ChatItem.Alert)?.messageId == messageId
    }.takeIf { it >= 0 }

    private fun renderComposer(state: ChatUiState) = with(binding) {
        if (chatInput.text?.toString() != state.draft) chatInput.setText(state.draft)
        chatSend.setVisible(state.canSend)
        chatMic.setVisible(!state.canSend)
    }

    private fun renderPending(state: ChatUiState) {
        binding.chatPendingScroll.setVisible(state.pending.isNotEmpty())
        binding.chatPending.removeAllViews()
        state.pending.forEach { binding.chatPending.addView(pendingChip(it)) }
    }

    private fun renderRecording(recording: RecordingState) = with(binding) {
        val active = recording as? RecordingState.Active
        chatRecording.setVisible(active != null)
        chatCompose.setVisible(active == null)
        chatMic.isPressed = active != null
        active?.let { chatRecordingTime.text = DateUtils.formatElapsedTime(it.elapsedMillis / MILLIS_PER_SECOND) }
    }

    private fun pendingChip(attachment: LocalAttachment): Chip = Components.inputChip(binding.chatPending).apply {
        text = pendingLabel(attachment)
        isCloseIconVisible = true
        closeIconContentDescription = getString(R.string.chat_attachment_remove, text)
        setOnCloseIconClickListener { dispatch(ChatIntent.RemovePending(attachment.path)) }
    }

    private fun pendingLabel(attachment: LocalAttachment): String = when (attachment.kind) {
        ChatAttachmentKind.IMAGE -> getString(R.string.chat_attach_photo)
        ChatAttachmentKind.VOICE -> getString(R.string.chat_voice_label)
        ChatAttachmentKind.FILE -> attachment.name
    }

    private fun onAttachChoice(itemId: Int) {
        when (itemId) {
            R.id.chat_attach_photo -> pickImage.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            else -> pickFile.launch(FILE_TYPES)
        }
    }

    internal fun startRecording(): Boolean {
        val granted = ContextCompat.checkSelfPermission(requireContext(), Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return false.also { askMicrophone.launch(Manifest.permission.RECORD_AUDIO) }
        dispatch(ChatIntent.StartRecording)
        return viewModel.state.value.recording is RecordingState.Active
    }

    private fun renderReply(state: ChatUiState) {
        val quote = state.replyTo
        binding.chatReplyBar.setVisible(quote != null)
        quote ?: return
        binding.chatReplyAuthor.setText(chatAuthor(quote.role))
        binding.chatReplyText.text = ChatMarkdown(MarkdownStyle(0, 0, 0, 0)).render(quote.text).toString()
    }

    internal fun replyTo(bubble: ChatItem.Bubble) {
        dispatch(ChatIntent.ReplyTo(bubble))
        binding.chatInput.requestFocus()
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
        adapter.bubbleAt(position)?.takeIf { it.quotable }?.let(::replyTo)
        adapter.notifyItemChanged(position)
    }

    internal fun showMenu(bubble: ChatItem.Bubble, anchor: View) {
        val popup = HomePopupMenu(anchor, R.menu.chat_message_actions) { onMenuAction(bubble, it) }
        popup.menu.findItem(R.id.chat_action_reply).isVisible = bubble.quotable
        popup.menu.findItem(R.id.chat_action_copy).isVisible = bubble.text.isNotEmpty()
        popup.menu.findItem(R.id.chat_action_retry).isVisible = bubble.retryable()
        popup.menu.findItem(R.id.chat_action_delete).isVisible = bubble.messageId == null
        popup.show()
    }

    private fun onMenuAction(bubble: ChatItem.Bubble, itemId: Int) {
        when (itemId) {
            R.id.chat_action_reply -> replyTo(bubble)
            R.id.chat_action_copy -> copy(bubble.text)
            R.id.chat_action_retry -> dispatch(ChatIntent.Retry(bubble))
            R.id.chat_action_delete -> dispatch(ChatIntent.Discard(bubble))
            else -> Unit
        }
    }

    private fun copy(text: String) {
        val clipboard = requireContext().getSystemService<ClipboardManager>() ?: return
        clipboard.setPrimaryClip(ClipData.newPlainText(getString(R.string.chat_title), text))
        showMessage(R.string.chat_copied)
    }

    internal fun loadImage(media: BubbleMedia, target: ImageView, width: Int) {
        target.tag = media.key
        target.setImageDrawable(null)
        viewLifecycleOwner.lifecycleScope.launch { viewModel.thumbnail(media, width).let { if (target.tag == media.key) target.setImageBitmap(it) } }
    }

    internal fun loadWave(media: BubbleMedia, target: VoiceWaveView) {
        target.setLevels(null)
        viewLifecycleOwner.lifecycleScope.launch { viewModel.waveLevels(media).let { if (target.tag == media.key) target.setLevels(it) } }
    }

    internal fun openImage(media: BubbleMedia) {
        val view = ImageView(requireContext()).apply { adjustViewBounds = true }
        loadImage(media, view, resources.displayMetrics.widthPixels)
        MaterialAlertDialogBuilder(requireContext()).setView(view).show()
    }

    internal fun openLink(url: String) {
        runCatching { startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
    }

    private fun ChatItem?.isMine(): Boolean = (this as? ChatItem.Bubble)?.mine == true

    private inner class Callbacks : ChatAdapterCallbacks() {
        override fun onBubbleMenu(bubble: ChatItem.Bubble, anchor: View) = showMenu(bubble, anchor)

        override fun onRetry(bubble: ChatItem.Bubble) = dispatch(ChatIntent.Retry(bubble))

        override fun onVoice(media: BubbleMedia) = dispatch(ChatIntent.ToggleVoice(media))

        override fun onImage(media: BubbleMedia) = openImage(media)

        override fun bindImage(media: BubbleMedia, target: ImageView) = loadImage(media, target, target.layoutParams.width)

        override fun bindWave(media: BubbleMedia, target: VoiceWaveView) = loadWave(media, target)

        override fun onAlertReply(alert: ChatItem.Alert) = replyTo(alert.asBubble())

        override fun onLink(url: String) = openLink(url)
    }

    private inner class Recording : RecordGesture {
        override fun onRecordStart(): Boolean = startRecording()

        override fun onRecordSend() = dispatch(ChatIntent.FinishRecording(send = true))

        override fun onRecordCancel() = dispatch(ChatIntent.FinishRecording(send = false))
    }

    private inner class ScrollWatcher : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
            binding.chatScrollDown.setVisible(!isAtEnd())
            val first = layoutManager.findFirstVisibleItemPosition()
            if (first in 0..OLDER_THRESHOLD) dispatch(ChatIntent.LoadOlder)
        }
    }

    private companion object {
        const val OLDER_THRESHOLD = 3
        const val MILLIS_PER_SECOND = 1000L
        val FILE_TYPES = arrayOf("text/plain", "text/markdown", "text/csv", "application/json")
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
