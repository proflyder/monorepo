package dev.proflyder.home.feature.settings.presentation

import android.graphics.Typeface
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.core.view.ViewCompat
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.ItemHistoryDayBinding
import dev.proflyder.home.feature.settings.impl.databinding.ItemNotificationHistoryBinding
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

internal interface NotificationHistoryAdapterCallback : AdapterCallback<HistoryRow>

internal typealias HistoryHolder = BaseViewHolder<HistoryRow, ViewBinding, NotificationHistoryAdapterCallback>

internal class NotificationHistoryAdapter(
    callback: NotificationHistoryAdapterCallback,
) : BaseListAdapter<HistoryRow, ViewBinding, NotificationHistoryAdapterCallback>(callback, KeyedDiff(HistoryRow::key)),
    SectionedAdapter {
    override fun viewTypeOf(item: HistoryRow): Int = if (item is HistoryRow.Day) DAY else ENTRY

    override fun isSectionHeader(position: Int): Boolean = getItem(position) is HistoryRow.Day

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): HistoryHolder =
        if (viewType == DAY) {
            DayHolder(ItemHistoryDayBinding.inflate(inflater, parent, false), callback)
        } else {
            EntryHolder(ItemNotificationHistoryBinding.inflate(inflater, parent, false), callback)
        }

    private class DayHolder(
        private val views: ItemHistoryDayBinding,
        callback: NotificationHistoryAdapterCallback,
    ) : HistoryHolder(views, callback) {
        init {
            ViewCompat.setAccessibilityHeading(views.root, true)
        }

        override fun bind(item: HistoryRow) {
            val day = item as? HistoryRow.Day ?: return
            views.historyDayTitle.text = when (day.day) {
                HistoryDay.TODAY -> views.root.context.getString(R.string.history_day_today)
                HistoryDay.YESTERDAY -> views.root.context.getString(R.string.history_day_yesterday)
                HistoryDay.EARLIER -> day.date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG))
            }
        }
    }

    private class EntryHolder(
        private val views: ItemNotificationHistoryBinding,
        callback: NotificationHistoryAdapterCallback,
    ) : HistoryHolder(views, callback) {

        override fun bind(item: HistoryRow) {
            val row = item as? HistoryRow.Item ?: return
            val entry = row.entry
            val open = row.target != null
            views.historyItemTitle.text = entry.title.ifBlank { views.root.context.getString(fallbackTitle(entry.type)) }
            views.historyItemBody.text = entry.body
            views.historyItemTime.text = HistoryFormat.relative(entry.createdUnix * MILLIS_PER_SECOND)
            views.historyItemIcon.setImageResource(icon(entry.type))
            views.historyItemIcon.contentDescription = views.root.context.getString(label(entry.type))
            views.historyItemChevron.setVisible(open)
            views.historyItemUnread.setVisible(!entry.read)
            views.historyItemTitle.setTypeface(null, if (entry.read) Typeface.NORMAL else Typeface.BOLD)
            if (open) forwardRowClicks() else clearClicks()
            views.root.isClickable = open
            views.root.isFocusable = open
        }

        private fun clearClicks() {
            views.root.setOnClickListener(null)
            views.root.setOnLongClickListener(null)
        }
    }

    private companion object {
        const val DAY = 0
        const val ENTRY = 1
        const val MILLIS_PER_SECOND = 1_000L
    }
}

internal fun icon(type: NotificationHistoryType): Int = when (type) {
    NotificationHistoryType.NOTIFY -> R.drawable.settings_ic_notify
    NotificationHistoryType.FIND -> R.drawable.settings_ic_find
}

internal fun fallbackTitle(type: NotificationHistoryType): Int = when (type) {
    NotificationHistoryType.NOTIFY -> R.string.history_type_notify
    NotificationHistoryType.FIND -> R.string.history_type_find
}

internal fun label(type: NotificationHistoryType): Int = when (type) {
    NotificationHistoryType.NOTIFY -> R.string.history_type_notify
    NotificationHistoryType.FIND -> R.string.history_type_find
}
