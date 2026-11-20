package dev.proflyder.home.feature.chat.presentation

import android.content.res.ColorStateList
import android.text.SpannableStringBuilder
import dev.proflyder.home.core.model.ChatAlert
import android.text.Spanned
import android.text.method.LinkMovementMethod
import android.text.style.URLSpan
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.view.animation.AlphaAnimation
import android.widget.ImageView
import android.text.format.DateUtils
import android.text.format.Formatter
import android.content.Context
import dev.proflyder.home.core.model.ChatAttachmentKind
import android.view.animation.Animation
import androidx.constraintlayout.widget.ConstraintLayout
import androidx.core.text.getSpans
import androidx.recyclerview.widget.DiffUtil
import com.google.android.material.color.MaterialColors
import com.google.android.material.shape.ShapeAppearanceModel
import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatAlertBinding
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatBubbleBinding
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatDayBinding
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatOlderBinding
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatTypingBinding
import com.google.android.material.R as MaterialR
import dev.proflyder.home.core.theme.R as ThemeR
import androidx.core.view.updateLayoutParams

internal open class ChatAdapterCallbacks : BaseAdapterCallback<ChatItem>() {
    open fun onBubbleMenu(bubble: ChatItem.Bubble, anchor: View) = Unit

    open fun onRetry(bubble: ChatItem.Bubble) = Unit

    open fun onVoice(media: BubbleMedia) = Unit

    open fun onImage(media: BubbleMedia) = Unit

    open fun bindImage(media: BubbleMedia, target: ImageView) = Unit

    open fun bindWave(media: BubbleMedia, target: VoiceWaveView) = Unit

    open fun onAlertReply(alert: ChatItem.Alert) = Unit

    open fun onLink(url: String) = Unit
}

private typealias ChatHolder = BaseViewHolder<ChatItem, ViewBinding, ChatAdapterCallbacks>

private object ChatDiff : DiffUtil.ItemCallback<ChatItem>() {
    override fun areItemsTheSame(oldItem: ChatItem, newItem: ChatItem): Boolean = oldItem.key == newItem.key

    override fun areContentsTheSame(oldItem: ChatItem, newItem: ChatItem): Boolean = oldItem == newItem
}

internal class ChatAdapter(
    callback: ChatAdapterCallbacks,
) : BaseListAdapter<ChatItem, ViewBinding, ChatAdapterCallbacks>(callback, ChatDiff) {

    override fun viewTypeOf(item: ChatItem): Int = when (item) {
        is ChatItem.Bubble -> TYPE_BUBBLE
        is ChatItem.Alert -> TYPE_ALERT
        is ChatItem.Day -> TYPE_DAY
        ChatItem.Typing -> TYPE_TYPING
        ChatItem.Older -> TYPE_OLDER
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ChatHolder =
        when (viewType) {
            TYPE_DAY -> DayHolder(ItemChatDayBinding.inflate(inflater, parent, false), callback)
            TYPE_TYPING -> TypingHolder(ItemChatTypingBinding.inflate(inflater, parent, false), callback)
            TYPE_ALERT -> AlertHolder(ItemChatAlertBinding.inflate(inflater, parent, false), callback)
            TYPE_OLDER -> OlderHolder(ItemChatOlderBinding.inflate(inflater, parent, false), callback)
            else -> BubbleHolder(ItemChatBubbleBinding.inflate(inflater, parent, false), callback)
        }

    fun bubbleAt(position: Int): ChatItem.Bubble? = currentList.getOrNull(position) as? ChatItem.Bubble

    private class DayHolder(
        private val views: ItemChatDayBinding,
        callback: ChatAdapterCallbacks,
    ) : ChatHolder(views, callback) {
        override fun bind(item: ChatItem) {
            val day = item as ChatItem.Day
            views.chatDay.text = chatDay(views.root.context, day.date)
        }
    }

    private class AlertHolder(
        private val views: ItemChatAlertBinding,
        callback: ChatAdapterCallbacks,
    ) : ChatHolder(views, callback) {
        init {
            views.chatAlertReply.setOnClickListener { callback.onAlertReply(item as ChatItem.Alert) }
            views.chatAlertRunbook.setOnClickListener { callback.onLink((item as ChatItem.Alert).alert.runbookUrl) }
            views.chatAlertCard.setOnLongClickListener { openMenu() }
        }

        private fun openMenu(): Boolean {
            callback.onBubbleMenu((item as ChatItem.Alert).asBubble(), views.chatAlertCard)
            return true
        }

        override fun bind(item: ChatItem) {
            val card = (item as ChatItem.Alert).alert
            val context = views.root.context
            val (stroke, accent) = alertColors(card)
            val accentColor = MaterialColors.getColor(views.chatAlertCard, accent)
            views.chatAlertCard.strokeColor = MaterialColors.getColor(views.chatAlertCard, stroke)
            views.chatAlertDot.backgroundTintList = ColorStateList.valueOf(accentColor)
            views.chatAlertState.setTextColor(accentColor)
            views.chatAlertName.text = card.name
            views.chatAlertState.text = context.getString(alertState(card))
            views.chatAlertSummary.text = card.summary.ifEmpty { card.name }
            views.chatAlertDescription.setVisible(card.description.isNotEmpty())
            views.chatAlertDescription.text = card.description
            views.chatAlertWhere.text = alertWhere(card)
            views.chatAlertTime.text = alertTime(context, card)
            views.chatAlertRunbook.setVisible(card.runbookUrl.isNotEmpty())
            views.chatAlertReply.setVisible(!card.resolved)
            views.chatAlertRunbook.updateLayoutParams<ViewGroup.MarginLayoutParams> {
                marginStart = if (card.resolved) 0 else views.root.resources.getDimensionPixelSize(R.dimen.chat_alert_button_gap)
            }
        }
    }

    private class OlderHolder(
        views: ItemChatOlderBinding,
        callback: ChatAdapterCallbacks,
    ) : ChatHolder(views, callback) {
        override fun bind(item: ChatItem) = Unit
    }

    private class TypingHolder(
        private val views: ItemChatTypingBinding,
        callback: ChatAdapterCallbacks,
    ) : ChatHolder(views, callback) {
        override fun bind(item: ChatItem) {
            listOf(views.chatTypingDot1, views.chatTypingDot2, views.chatTypingDot3)
                .forEachIndexed { index, dot -> dot.startAnimation(pulse(index)) }
        }

        private fun pulse(index: Int): Animation = AlphaAnimation(1f, DIM_ALPHA).apply {
            duration = PULSE_MILLIS
            startOffset = index * PULSE_STEP_MILLIS
            repeatMode = Animation.REVERSE
            repeatCount = Animation.INFINITE
        }
    }

    private class BubbleHolder(
        private val views: ItemChatBubbleBinding,
        callback: ChatAdapterCallbacks,
    ) : ChatHolder(views, callback) {
        private val markdown = ChatMarkdown(markdownStyle(views.root))

        init {
            views.chatImage.outlineProvider = RoundedOutline(views.root.resources.getDimension(R.dimen.chat_image_radius))
            views.chatImage.clipToOutline = true
            views.chatBubble.setOnLongClickListener { openMenu() }
            views.chatText.setOnLongClickListener { openMenu() }
            views.chatRetry.setOnClickListener { callback.onRetry(item as ChatItem.Bubble) }
            views.chatVoicePlay.setOnClickListener { firstMedia(ChatAttachmentKind.VOICE)?.let(callback::onVoice) }
            views.chatImage.setOnClickListener { firstMedia(ChatAttachmentKind.IMAGE)?.let(callback::onImage) }
            views.chatImage.setOnLongClickListener { openMenu() }
        }

        private fun firstMedia(kind: ChatAttachmentKind): BubbleMedia? =
            (item as ChatItem.Bubble).media.firstOrNull { it.kind == kind }

        private fun openMenu(): Boolean {
            callback.onBubbleMenu(item as ChatItem.Bubble, views.chatBubble)
            return true
        }

        override fun bind(item: ChatItem) {
            val bubble = item as ChatItem.Bubble
            align(bubble.mine)
            paint(bubble)
            bindText(bubble)
            bindQuote(bubble)
            bindMedia(bubble)
            bindMeta(bubble)
            bindFailure(bubble)
        }

        private fun align(mine: Boolean) {
            val params = views.chatBubble.layoutParams as ConstraintLayout.LayoutParams
            params.horizontalBias = if (mine) 1f else 0f
            views.chatBubble.layoutParams = params
            val radius = views.root.resources.getDimension(R.dimen.chat_bubble_radius)
            val tail = views.root.resources.getDimension(R.dimen.chat_bubble_tail_radius)
            views.chatBubble.shapeAppearanceModel = ShapeAppearanceModel.builder()
                .setAllCornerSizes(radius)
                .apply { if (mine) setBottomRightCornerSize(tail) else setBottomLeftCornerSize(tail) }
                .build()
        }

        private fun paint(bubble: ChatItem.Bubble) {
            val (background, foreground) = bubbleColors(bubble)
            val card = views.chatBubble
            card.setCardBackgroundColor(MaterialColors.getColor(card, background))
            val text = MaterialColors.getColor(card, foreground)
            views.chatText.setTextColor(text)
            views.chatQuoteText.setTextColor(text)
            views.chatText.setLinkTextColor(MaterialColors.getColor(card, androidx.appcompat.R.attr.colorPrimary))
        }

        private fun bindText(bubble: ChatItem.Bubble) {
            val body = markdown.render(bubble.text)
            val shown = if (bubble.state == BubbleState.STREAMING) SpannableStringBuilder(body).append(STREAMING_CARET) else body
            views.chatText.setVisible(bubble.text.isNotEmpty())
            views.chatText.text = shown
            val hasLinks = (body as? Spanned)?.getSpans<URLSpan>()?.isNotEmpty() == true
            views.chatText.movementMethod = if (hasLinks) LinkMovementMethod.getInstance() else null
        }

        private fun bindQuote(bubble: ChatItem.Bubble) {
            val quote = bubble.quote
            views.chatQuote.setVisible(quote != null)
            quote ?: return
            views.chatQuoteAuthor.setText(chatAuthor(quote.role))
            views.chatQuoteText.text = markdown.render(quote.text).toString()
        }

        private fun bindMedia(bubble: ChatItem.Bubble) {
            val image = bubble.media.firstOrNull { it.kind == ChatAttachmentKind.IMAGE }
            val voice = bubble.media.firstOrNull { it.kind == ChatAttachmentKind.VOICE }
            val file = bubble.media.firstOrNull { it.kind == ChatAttachmentKind.FILE }
            views.chatImage.setVisible(image != null)
            views.chatBubble.layoutParams = (views.chatBubble.layoutParams as ConstraintLayout.LayoutParams).apply {
                width = if (image != null) ConstraintLayout.LayoutParams.MATCH_CONSTRAINT else ConstraintLayout.LayoutParams.WRAP_CONTENT
            }
            image?.let { callback.bindImage(it, views.chatImage) }
            views.chatVoice.setVisible(voice != null)
            voice?.let { bindVoice(it, bubble.playback) }
            views.chatFile.setVisible(file != null)
            views.chatFileName.text = file?.let { fileLabel(views.root.context, it) }
            bindTranscript(bubble, voice)
        }

        private fun bindVoice(voice: BubbleMedia, playback: VoicePlayback?) {
            val playing = playback?.playing == true
            views.chatVoicePlay.setIconResource(if (playing) R.drawable.chat_ic_pause else R.drawable.chat_ic_play)
            views.chatVoicePlay.contentDescription = views.root.context.getString(
                if (playing) R.string.chat_voice_pause else R.string.chat_voice_play,
            )
            if (views.chatVoiceWave.tag != voice.key) callback.bindWave(voice, views.chatVoiceWave)
            views.chatVoiceWave.tag = voice.key
            views.chatVoiceWave.setProgress(playback?.progress ?: 0f)
            val shown = playback?.takeIf { it.positionMs > 0 }?.positionMs ?: voice.durationMs ?: 0L
            views.chatVoiceDuration.text = DateUtils.formatElapsedTime(shown / MILLIS_PER_SECOND)
        }

        private fun bindTranscript(bubble: ChatItem.Bubble, voice: BubbleMedia?) {
            val transcript = bubble.transcript
            val waiting = voice != null && transcript == null && bubble.mine && bubble.state != BubbleState.FAILED
            views.chatTranscript.setVisible(transcript != null || waiting)
            views.chatTranscript.text = transcript ?: views.root.context.getString(R.string.chat_transcript_pending)
        }

        private fun bindMeta(bubble: ChatItem.Bubble) {
            views.chatTime.text = chatTime(views.root.context, bubble.atMillis)
            val icon = statusIcon(bubble)
            views.chatStatus.setVisible(icon != null)
            icon?.let(views.chatStatus::setImageResource)
            views.chatStatus.contentDescription = statusText(bubble)?.let(views.root.context::getString)
        }

        private fun bindFailure(bubble: ChatItem.Bubble) {
            val failed = bubble.state == BubbleState.FAILED
            views.chatFailure.setVisible(failed)
            views.chatFailureText.setText(chatErrorText(bubble.errorKey))
            views.chatRetry.setVisible(failed && bubble.retryable())
        }
    }

    private companion object {
        const val TYPE_BUBBLE = 0
        const val TYPE_DAY = 1
        const val TYPE_TYPING = 2
        const val TYPE_OLDER = 3
        const val TYPE_ALERT = 4
        const val DIM_ALPHA = 0.25f
        const val PULSE_MILLIS = 450L
        const val PULSE_STEP_MILLIS = 150L
        const val STREAMING_CARET = " ▍"
        const val MILLIS_PER_SECOND = 1000L
    }
}

internal fun ChatItem.Bubble.retryable(): Boolean = state == BubbleState.FAILED && when {
    messageId == null -> errorKey != "chat_text_empty" && errorKey != "chat_text_too_long"
    else -> !mine
}

internal fun bubbleColors(bubble: ChatItem.Bubble): Pair<Int, Int> = when {
    bubble.mine -> MaterialR.attr.colorPrimaryContainer to MaterialR.attr.colorOnPrimaryContainer
    bubble.kind == ChatKind.ALERT -> MaterialR.attr.colorErrorContainer to MaterialR.attr.colorOnErrorContainer
    else -> MaterialR.attr.colorSurfaceContainerHigh to MaterialR.attr.colorOnSurface
}

internal fun statusIcon(bubble: ChatItem.Bubble): Int? = when {
    !bubble.mine -> null
    bubble.state == BubbleState.QUEUED -> R.drawable.chat_ic_clock
    bubble.state == BubbleState.FAILED -> R.drawable.chat_ic_error
    else -> R.drawable.chat_ic_check
}

internal fun statusText(bubble: ChatItem.Bubble): Int? = when {
    !bubble.mine -> null
    bubble.state == BubbleState.QUEUED -> R.string.chat_state_queued
    bubble.state == BubbleState.FAILED -> R.string.chat_state_failed
    else -> R.string.chat_state_sent
}

internal fun markdownStyle(view: View): MarkdownStyle = MarkdownStyle(
    codeBackground = MaterialColors.getColor(view, MaterialR.attr.colorSurfaceContainerHighest),
    codeText = MaterialColors.getColor(view, MaterialR.attr.colorOnSurface),
    linkColor = MaterialColors.getColor(view, androidx.appcompat.R.attr.colorPrimary),
    listIndent = view.resources.getDimensionPixelSize(R.dimen.chat_list_indent),
)

internal fun fileLabel(context: Context, media: BubbleMedia): String =
    "${media.name} · ${Formatter.formatShortFileSize(context, media.size)}"

internal class RoundedOutline(private val radius: Float) : android.view.ViewOutlineProvider() {
    override fun getOutline(view: View, outline: android.graphics.Outline) {
        outline.setRoundRect(0, 0, view.width, view.height, radius)
    }
}

internal fun alertColors(card: ChatAlert): Pair<Int, Int> = when {
    card.resolved -> MaterialR.attr.colorOutlineVariant to ThemeR.attr.colorSuccess
    card.severity == "critical" -> androidx.appcompat.R.attr.colorError to androidx.appcompat.R.attr.colorError
    else -> ThemeR.attr.colorWarning to ThemeR.attr.colorWarning
}

internal fun alertState(card: ChatAlert): Int = when {
    card.resolved -> R.string.chat_alert_resolved
    card.severity == "critical" -> R.string.chat_alert_critical
    card.severity == "warning" -> R.string.chat_alert_warning
    else -> R.string.chat_alert_firing
}

internal fun alertWhere(card: ChatAlert): String =
    listOf(card.namespace, card.target).filter { it.isNotEmpty() }.joinToString(" / ")

internal fun alertTime(context: Context, card: ChatAlert): String {
    val start = chatTime(context, card.startsAtMillis)
    val end = card.endsAtMillis
    return when (end) {
        null -> context.getString(R.string.chat_alert_since, start)
        else -> context.getString(R.string.chat_alert_span, start, chatTime(context, end))
    }
}
