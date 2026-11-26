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
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.chip.Chip
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.Components
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.designsystem.glass.bindGlassBackdrop
import dev.proflyder.home.core.model.ChatSession
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.chat.presentation.sessions.SessionMenu
import dev.proflyder.home.feature.chat.presentation.sessions.displayTitle
import android.view.Gravity
import dev.proflyder.home.core.ui.ContentInsets
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.core.ui.setShown
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.chat.ChatFocus
import dev.proflyder.home.feature.chat.data.AttachmentSource
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatBinding
import kotlinx.coroutines.launch

internal class ChatFragment : ScreenFragment<FragmentChatBinding, ChatUiState, ChatIntent, ChatEvent>(
    FragmentChatBinding.Companion::inflate,
) {
    override val viewModel: ChatViewModel by screenViewModel()

    private val adapter = ChatAdapter(Callbacks())
    internal val layoutManager: LinearLayoutManager get() = binding.chatList.layoutManager as LinearLayoutManager
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

    override fun contentInsets(): ContentInsets = ContentInsets(bottom = false)

    override fun onViewReady(binding: FragmentChatBinding, savedInstanceState: Bundle?) = with(binding) {
        chatList.adapter = adapter
        chatList.addOnScrollListener(ScrollWatcher())
        SwipeReply(chatList, ::isQuotable, ::onSwipeReply).attach()
        chatPanel.pinAboveKeyboard(chatList)
        chatHeader.bindGlassBackdrop(chatList)
        chatPanel.bindGlassBackdrop(chatList)
        chatBack.setOnClickListener { navigate(Route.Back) }
        chatActions.setOnClickListener { showSessionMenu() }
        chatStop.setOnClickListener { dispatch(ChatIntent.Stop) }
        chatList.addOnLayoutChangeListener(StatesFollowList(chatStates))
        chatInput.doAfterTextChanged { dispatch(ChatIntent.InputChanged(it?.toString().orEmpty())) }
        chatSend.setOnClickListener { dispatch(ChatIntent.Send) }
        chatMic.setOnTouchListener(RecordTouch(Recording(), resources.getDimension(R.dimen.chat_swipe_reply_distance)))
        chatAttach.setOnClickListener { HomePopupMenu(it, R.menu.chat_attach, ::onAttachChoice).show() }
        chatReplyCancel.setOnClickListener { dispatch(ChatIntent.CancelReply) }
        chatScrollDown.setOnClickListener { scrollToEnd(smooth = true) }
        chatErrorRetry.setOnClickListener { dispatch(ChatIntent.Refresh) }
        render({ ChatStatus(it.loaded, it.failure, it.items.isEmpty()) }, ::renderStatus)
        render({ Composer(it.draft, it.canSend, it.stoppableReplyId != null) }, ::renderComposer)
        render(ChatUiState::replyTo, ::renderReply)
        render(ChatUiState::pending, ::renderPending)
        render({ it.replyTo != null || it.pending.isNotEmpty() }, chatExtras::setVisible)
        render(ChatUiState::session, ::renderSession)
        render({ HeaderStatus(it.activeTool, it.stoppableReplyId != null) }, ::renderStatusLine)
        render(ChatUiState::recording, ::renderRecording)
        render(ChatUiState::items, ::renderItems)
    }

    override fun onBindingDestroyed() {
        binding.chatList.adapter = null
    }

    private fun renderStatus(status: ChatStatus) = with(binding) {
        chatProgress.setVisible(!status.loaded && status.failure == null && status.empty)
        chatError.setVisible(!status.loaded && status.failure != null && status.empty)
        chatErrorText.text = status.failure?.userMessage(requireContext())
        chatEmpty.setVisible(status.loaded && status.empty)
    }

    private fun renderItems(items: List<ChatItem>) {
        val follow = isAtEnd() || items.size > lastCount && items.lastOrNull().isMine()
        lastCount = items.size
        adapter.submitList(items) { afterSubmit(follow) }
    }

    override fun onEvent(event: ChatEvent) {
        when (event) {
            is ChatEvent.Failure -> showFailure(event.failure)
            is ChatEvent.Rejected -> showError(chatErrorText(event.key))
            ChatEvent.Closed -> navigate(Route.Back)
        }
    }

    override fun onStart() {
        super.onStart()
        ChatFocus.setVisibleSession(viewModel.sessionId)
    }

    override fun onStop() {
        ChatFocus.setVisibleSession(null)
        super.onStop()
    }

    private fun renderSession(session: ChatSession?) {
        binding.chatTitle.text = session?.displayTitle(requireContext()) ?: getString(R.string.chat_session_untitled)
        binding.chatActions.isEnabled = session != null
    }

    private fun renderStatusLine(status: HeaderStatus) {
        val text = when {
            status.tool != null -> getString(R.string.chat_tool_running, status.tool)
            status.answering -> getString(R.string.chat_session_preview_typing)
            else -> null
        }
        binding.chatSubtitle.text = text
        binding.chatSubtitle.setVisible(text != null)
    }

    private fun showSessionMenu() {
        val session = viewModel.state.value.session ?: return
        SessionMenu(binding.chatActions, session) { dispatch(ChatIntent.Act(it)) }.show(Gravity.END)
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

    private fun renderComposer(composer: Composer) = with(binding) {
        if (chatInput.text?.toString() != composer.draft) chatInput.setText(composer.draft)
        chatSend.setVisible(composer.canSend)
        chatStop.setVisible(!composer.canSend && composer.answering)
        chatMic.setVisible(!composer.canSend && !composer.answering)
    }

    private fun renderPending(pending: List<LocalAttachment>) {
        binding.chatPendingScroll.setVisible(pending.isNotEmpty())
        binding.chatPending.removeAllViews()
        pending.forEach { binding.chatPending.addView(pendingChip(it)) }
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

    private fun renderReply(quote: ChatQuote?) {
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

    private fun isQuotable(position: Int): Boolean = adapter.quoteAt(position) != null

    private fun onSwipeReply(position: Int) {
        adapter.quoteAt(position)?.let(::replyTo)
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

        override fun onAlertReply(alert: ChatItem.Alert) {
            alert.quote()?.let(::replyTo)
        }

        override fun onLink(url: String) = openLink(url)
    }

    private inner class Recording : RecordGesture {
        override fun onRecordStart(): Boolean = startRecording()

        override fun onRecordSend() = dispatch(ChatIntent.FinishRecording(send = true))

        override fun onRecordCancel() = dispatch(ChatIntent.FinishRecording(send = false))
    }

    private inner class ScrollWatcher : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
            binding.chatScrollDown.setShown(!isAtEnd())
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

internal data class ChatStatus(val loaded: Boolean, val failure: RemoteFailure?, val empty: Boolean)

internal data class Composer(val draft: String, val canSend: Boolean, val answering: Boolean)

internal data class HeaderStatus(val tool: String?, val answering: Boolean)

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
