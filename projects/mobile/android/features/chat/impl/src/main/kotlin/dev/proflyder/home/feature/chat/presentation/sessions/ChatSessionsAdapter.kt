package dev.proflyder.home.feature.chat.presentation.sessions

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.feature.chat.impl.R
import java.text.NumberFormat
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatSessionArchiveBinding
import dev.proflyder.home.feature.chat.impl.databinding.ItemChatSessionBinding
import dev.proflyder.home.feature.chat.presentation.chatListTime

private typealias RowCallback = AdapterCallback<SessionRow>

private typealias RowHolder = BaseViewHolder<SessionRow, ViewBinding, RowCallback>

private val RowDiff = KeyedDiff<SessionRow>(key = { it.key })

internal interface SessionRowListener {
    fun onOpen(row: SessionRow.Item)

    fun onActions(row: SessionRow.Item, anchor: View)

    fun onArchive()
}

internal class ChatSessionsAdapter(
    private val listener: SessionRowListener,
) : BaseListAdapter<SessionRow, ViewBinding, RowCallback>(BaseAdapterCallback<SessionRow>(), RowDiff) {
    override fun viewTypeOf(item: SessionRow): Int = when (item) {
        is SessionRow.Archive -> TYPE_ARCHIVE
        is SessionRow.Item -> TYPE_SESSION
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): RowHolder = when (viewType) {
        TYPE_ARCHIVE -> ArchiveHolder(ItemChatSessionArchiveBinding.inflate(inflater, parent, false))
        else -> SessionHolder(ItemChatSessionBinding.inflate(inflater, parent, false))
    }

    private inner class ArchiveHolder(private val views: ItemChatSessionArchiveBinding) : RowHolder(views, BaseAdapterCallback<SessionRow>()) {
        override fun bind(item: SessionRow) {
            views.chatArchiveCount.text = NumberFormat.getIntegerInstance().format((item as SessionRow.Archive).count)
            views.root.setOnClickListener { listener.onArchive() }
        }
    }

    private inner class SessionHolder(private val views: ItemChatSessionBinding) : RowHolder(views, BaseAdapterCallback<SessionRow>()) {
        override fun bind(item: SessionRow) {
            val row = item as SessionRow.Item
            val context = views.root.context
            val session = row.session
            views.chatSessionTitle.text = session.displayTitle(context)
            views.chatSessionPreview.text = session.preview.describe(context)
            views.chatSessionTime.text = chatListTime(context, session.updatedAtMillis)
            views.chatSessionAvatar.text = session.initial(context)
            views.chatSessionAvatar.isActivated = true
            views.chatSessionPreview.setCompoundDrawablesRelativeWithIntrinsicBounds(0, 0, pinIcon(session.pinned), 0)
            views.root.setOnClickListener { listener.onOpen(row) }
            views.root.setOnLongClickListener { true.also { listener.onActions(row, views.root) } }
        }
    }

    private fun pinIcon(pinned: Boolean): Int = if (pinned) R.drawable.chat_ic_pin_small else 0

    private companion object {
        const val TYPE_ARCHIVE = 0
        const val TYPE_SESSION = 1
    }
}
