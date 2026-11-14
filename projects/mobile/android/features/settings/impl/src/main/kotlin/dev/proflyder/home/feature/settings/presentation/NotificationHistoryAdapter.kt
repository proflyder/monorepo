package dev.proflyder.home.feature.settings.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.push.NotificationHistoryEntry
import dev.proflyder.home.feature.push.NotificationHistoryType
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.ItemNotificationHistoryBinding

internal interface NotificationHistoryAdapterCallback : AdapterCallback<NotificationHistoryEntry>

private object HistoryDiff : DiffUtil.ItemCallback<NotificationHistoryEntry>() {
    override fun areItemsTheSame(oldItem: NotificationHistoryEntry, newItem: NotificationHistoryEntry): Boolean =
        oldItem.id == newItem.id

    override fun areContentsTheSame(oldItem: NotificationHistoryEntry, newItem: NotificationHistoryEntry): Boolean =
        oldItem == newItem
}

/** One received message per card: type icon, title, body, relative time. */
internal class NotificationHistoryAdapter(
    callback: NotificationHistoryAdapterCallback,
) : BaseListAdapter<NotificationHistoryEntry, ItemNotificationHistoryBinding, NotificationHistoryAdapterCallback>(
    callback,
    HistoryDiff,
) {
    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<NotificationHistoryEntry, ItemNotificationHistoryBinding, NotificationHistoryAdapterCallback> =
        Holder(ItemNotificationHistoryBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemNotificationHistoryBinding,
        callback: NotificationHistoryAdapterCallback,
    ) : BaseViewHolder<NotificationHistoryEntry, ItemNotificationHistoryBinding, NotificationHistoryAdapterCallback>(
        binding,
        callback,
    ) {
        init {
            forwardRowClicks()
        }

        override fun bind(item: NotificationHistoryEntry) {
            val context = binding.root.context
            binding.historyItemTitle.text = item.title
            binding.historyItemBody.text = item.body
            binding.historyItemTime.text = HistoryFormat.relative(item.receivedAtMillis)
            binding.historyItemIcon.setImageResource(icon(item.type))
            binding.historyItemIcon.contentDescription = context.getString(label(item.type))
            binding.historyItemChevron.setVisible(item.deepLink != null)
        }
    }
}

/** The type icon is the same shape the tray notification used. */
internal fun icon(type: NotificationHistoryType): Int = when (type) {
    NotificationHistoryType.NOTIFY -> R.drawable.settings_ic_notify
    NotificationHistoryType.FIND -> R.drawable.settings_ic_find
}

/** Spoken label of the row type; the row title is the message itself. */
internal fun label(type: NotificationHistoryType): Int = when (type) {
    NotificationHistoryType.NOTIFY -> R.string.history_type_notify
    NotificationHistoryType.FIND -> R.string.history_type_find
}
