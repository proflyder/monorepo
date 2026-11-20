package dev.proflyder.home.feature.chat.presentation

import android.Manifest
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.content.ClipData
import android.content.pm.PackageManager
import android.text.format.DateUtils
import android.widget.ImageView
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.google.android.material.chip.Chip
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.VoiceRecorder
import java.io.File
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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
    private lateinit var recorder: VoiceRecorder
    private lateinit var player: ChatVoicePlayer
    private var recordingTicker: Job? = null

    private val pickImage = registerForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri?.let(viewModel::attachImage)
    }

    private val pickFile = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(viewModel::attachFile)
    }

    private val askMicrophone = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (!granted) showError(R.string.chat_record_permission)
    }

    override fun onBindingCreated(binding: FragmentChatBinding, savedInstanceState: Bundle?) {
        adapter = ChatAdapter(Callbacks())
        layoutManager = LinearLayoutManager(requireContext()).apply { stackFromEnd = true }
        binding.chatList.layoutManager = layoutManager
        binding.chatList.adapter = adapter
        binding.chatList.addOnScrollListener(ScrollWatcher())
        ItemTouchHelper(SwipeToReply(::onSwipeReply)).attachToRecyclerView(binding.chatList)
        binding.chatPanel.pinAboveKeyboard(binding.chatList)
        binding.chatList.addOnLayoutChangeListener(StatesFollowList(binding.chatStates))
        recorder = VoiceRecorder(viewModel::newVoiceFile)
        player = ChatVoicePlayer(viewModel::onPlayback)
        binding.chatInput.doAfterTextChanged { renderSendButton() }
        binding.chatSend.setOnClickListener { send() }
        binding.chatMic.setOnTouchListener(RecordTouch(Recording(), resources.getDimension(R.dimen.chat_swipe_reply_distance)))
        binding.chatAttach.setOnClickListener { showAttachMenu(it) }
        viewModel.rejected.observe { showError(chatErrorText(it)) }
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
        renderPending(state)
        val follow = isAtEnd() || state.items.size > lastCount && state.items.lastOrNull().isMine()
        lastCount = state.items.size
        adapter.submitList(state.items) { if (follow) scrollToEnd(smooth = false) }
    }

    override fun onBindingDestroyed() {
        recorder.cancel()
        player.release()
        recordingTicker?.cancel()
    }

    private fun renderSendButton() {
        val canSend = !binding.chatInput.text.isNullOrBlank() || viewModel.state.value.pending.isNotEmpty()
        binding.chatSend.setVisible(canSend)
        binding.chatMic.setVisible(!canSend)
    }

    private fun renderPending(state: ChatUiState) {
        binding.chatPendingScroll.setVisible(state.pending.isNotEmpty())
        binding.chatPending.removeAllViews()
        state.pending.forEach { binding.chatPending.addView(pendingChip(it)) }
        renderSendButton()
    }

    private fun pendingChip(attachment: LocalAttachment): Chip = Chip(requireContext()).apply {
        text = pendingLabel(attachment)
        isCloseIconVisible = true
        closeIconContentDescription = getString(R.string.chat_attachment_remove, text)
        setOnCloseIconClickListener { viewModel.removePending(attachment.path) }
    }

    private fun pendingLabel(attachment: LocalAttachment): String = when (attachment.kind) {
        ChatAttachmentKind.IMAGE -> getString(R.string.chat_attach_photo)
        ChatAttachmentKind.VOICE -> getString(R.string.chat_voice_label)
        ChatAttachmentKind.FILE -> attachment.name
    }

    private fun showAttachMenu(anchor: View) {
        val popup = PopupMenu(requireContext(), anchor)
        popup.menu.add(0, ATTACH_PHOTO, 0, R.string.chat_attach_photo)
        popup.menu.add(0, ATTACH_FILE, 1, R.string.chat_attach_file)
        popup.setOnMenuItemClickListener { item -> onAttachChoice(item.itemId) }
        popup.show()
    }

    private fun onAttachChoice(itemId: Int): Boolean {
        when (itemId) {
            ATTACH_PHOTO -> pickImage.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            else -> pickFile.launch(FILE_TYPES)
        }
        return true
    }

    internal fun startRecording(): Boolean {
        val granted = ContextCompat.checkSelfPermission(requireContext(), Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return false.also { askMicrophone.launch(Manifest.permission.RECORD_AUDIO) }
        if (!recorder.start(viewLifecycleOwner.lifecycleScope)) return false
        player.release()
        showRecording(true)
        recordingTicker = viewLifecycleOwner.lifecycleScope.launch { tickRecording() }
        return true
    }

    private suspend fun tickRecording() {
        while (recorder.recording) {
            val elapsed = recorder.elapsedMillis()
            binding.chatRecordingTime.text = DateUtils.formatElapsedTime(elapsed / MILLIS_PER_SECOND)
            if (elapsed >= VoiceRecorder.MAX_MILLIS) finishRecording(send = true)
            delay(RECORDING_TICK_MILLIS)
        }
    }

    internal fun finishRecording(send: Boolean) {
        recordingTicker?.cancel()
        showRecording(false)
        if (!send) return recorder.cancel()
        viewLifecycleOwner.lifecycleScope.launch { recorder.stop()?.let(viewModel::sendVoice) }
    }

    private fun showRecording(recording: Boolean) {
        binding.chatRecording.setVisible(recording)
        binding.chatCompose.setVisible(!recording)
        binding.chatMic.isPressed = recording
        if (recording) binding.chatRecordingTime.text = DateUtils.formatElapsedTime(0)
    }

    internal fun toggleVoice(media: BubbleMedia) {
        viewLifecycleOwner.lifecycleScope.launch {
            viewModel.mediaFile(media)?.let { player.toggle(viewLifecycleOwner.lifecycleScope, media.key, it) }
        }
    }

    internal fun loadImage(media: BubbleMedia, target: ImageView) {
        target.tag = media.key
        target.setImageDrawable(null)
        viewLifecycleOwner.lifecycleScope.launch { showImage(media, target) }
    }

    private suspend fun showImage(media: BubbleMedia, target: ImageView) {
        val bitmap = viewModel.mediaFile(media)?.let { decodeThumbnail(it, target.layoutParams.width) }
        if (target.tag == media.key) target.setImageBitmap(bitmap)
    }

    internal fun openImage(media: BubbleMedia) {
        viewLifecycleOwner.lifecycleScope.launch { viewModel.mediaFile(media)?.let(::showFullImage) }
    }

    private fun showFullImage(file: File) {
        val view = ImageView(requireContext()).apply { adjustViewBounds = true }
        view.setImageBitmap(decodeThumbnail(file, resources.displayMetrics.widthPixels))
        MaterialAlertDialogBuilder(requireContext()).setView(view).show()
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

        override fun onVoice(media: BubbleMedia) = toggleVoice(media)

        override fun onImage(media: BubbleMedia) = openImage(media)

        override fun bindImage(media: BubbleMedia, target: ImageView) = loadImage(media, target)
    }

    private inner class Recording : RecordGesture {
        override fun onRecordStart(): Boolean = startRecording()

        override fun onRecordSend() = finishRecording(send = true)

        override fun onRecordCancel() = finishRecording(send = false)
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
        const val ATTACH_PHOTO = 1
        const val ATTACH_FILE = 2
        const val RECORDING_TICK_MILLIS = 250L
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

internal fun decodeThumbnail(file: File, targetWidth: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    val sample = generateSequence(1) { it * 2 }.first { bounds.outWidth / it <= maxOf(targetWidth, 1) * 2 }
    return BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })
}
