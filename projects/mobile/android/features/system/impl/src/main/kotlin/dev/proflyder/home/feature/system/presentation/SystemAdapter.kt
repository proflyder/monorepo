package dev.proflyder.home.feature.system.presentation

import android.content.res.ColorStateList
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.feature.system.impl.R
import dev.proflyder.home.feature.system.impl.databinding.ItemSystemCardBinding
import dev.proflyder.home.feature.system.impl.databinding.ItemSystemGroupBinding
import dev.proflyder.home.feature.system.impl.databinding.ItemSystemNoteBinding
import dev.proflyder.home.feature.system.impl.databinding.ItemSystemRowBinding
import dev.proflyder.home.feature.system.impl.databinding.ItemSystemStatusBinding

internal interface SystemAdapterCallback : AdapterCallback<SystemRow> {
    fun onSectionClick(section: SystemSection)
}

internal open class SystemAdapterCallbacks : BaseAdapterCallback<SystemRow>(), SystemAdapterCallback {
    override fun onSectionClick(section: SystemSection) = Unit
}

private typealias SystemHolder = BaseViewHolder<SystemRow, ViewBinding, SystemAdapterCallback>

private object SystemDiff : DiffUtil.ItemCallback<SystemRow>() {
    override fun areItemsTheSame(oldItem: SystemRow, newItem: SystemRow): Boolean = oldItem.key == newItem.key

    override fun areContentsTheSame(oldItem: SystemRow, newItem: SystemRow): Boolean = oldItem == newItem
}

internal fun levelColorAttr(level: HealthLevel): Int = when (level) {
    HealthLevel.OK -> androidx.appcompat.R.attr.colorPrimary
    HealthLevel.WARNING -> com.google.android.material.R.attr.colorTertiary
    HealthLevel.CRITICAL -> androidx.appcompat.R.attr.colorError
    HealthLevel.UNKNOWN -> com.google.android.material.R.attr.colorOutline
}

internal fun levelLabel(level: HealthLevel): Int = when (level) {
    HealthLevel.OK -> R.string.system_level_ok
    HealthLevel.WARNING -> R.string.system_level_warning
    HealthLevel.CRITICAL -> R.string.system_level_critical
    HealthLevel.UNKNOWN -> R.string.system_level_unknown
}

internal fun View.tintLevel(level: HealthLevel) {
    backgroundTintList = ColorStateList.valueOf(MaterialColors.getColor(this, levelColorAttr(level)))
    contentDescription = context.getString(levelLabel(level))
}

internal class SystemAdapter(
    callback: SystemAdapterCallback,
) : BaseListAdapter<SystemRow, ViewBinding, SystemAdapterCallback>(callback, SystemDiff) {

    override fun viewTypeOf(item: SystemRow): Int = when (item) {
        is SystemRow.Status -> TYPE_STATUS
        is SystemRow.Card -> TYPE_CARD
        is SystemRow.Group -> TYPE_GROUP
        is SystemRow.Item -> TYPE_ITEM
        is SystemRow.Note -> TYPE_NOTE
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): SystemHolder =
        when (viewType) {
            TYPE_STATUS -> StatusHolder(ItemSystemStatusBinding.inflate(inflater, parent, false), callback)
            TYPE_CARD -> CardHolder(ItemSystemCardBinding.inflate(inflater, parent, false), callback)
            TYPE_GROUP -> GroupHolder(ItemSystemGroupBinding.inflate(inflater, parent, false), callback)
            TYPE_NOTE -> NoteHolder(ItemSystemNoteBinding.inflate(inflater, parent, false), callback)
            else -> ItemHolder(ItemSystemRowBinding.inflate(inflater, parent, false), callback)
        }

    private class StatusHolder(
        private val views: ItemSystemStatusBinding,
        callback: SystemAdapterCallback,
    ) : SystemHolder(views, callback) {
        override fun bind(item: SystemRow) {
            val status = item as SystemRow.Status
            views.systemStatusLight.tintLevel(status.level)
            views.systemStatusTitle.text = status.title
            views.systemStatusSubtitle.text = status.subtitle
        }
    }

    private class CardHolder(
        private val views: ItemSystemCardBinding,
        callback: SystemAdapterCallback,
    ) : SystemHolder(views, callback) {
        override fun bind(item: SystemRow) {
            val card = item as SystemRow.Card
            views.systemCardLight.tintLevel(card.level)
            views.systemCardTitle.text = card.title
            views.systemCardLines.text = card.lines
            views.root.setOnClickListener { callback.onSectionClick(card.section) }
        }
    }

    private class GroupHolder(
        private val views: ItemSystemGroupBinding,
        callback: SystemAdapterCallback,
    ) : SystemHolder(views, callback) {
        override fun bind(item: SystemRow) {
            val group = item as SystemRow.Group
            views.systemGroupLight.tintLevel(group.level)
            views.systemGroupTitle.text = group.title
        }
    }

    private class ItemHolder(
        private val views: ItemSystemRowBinding,
        callback: SystemAdapterCallback,
    ) : SystemHolder(views, callback) {
        override fun bind(item: SystemRow) {
            val row = item as SystemRow.Item
            views.systemRowLight.tintLevel(row.level)
            views.systemRowTitle.text = row.title
            views.systemRowSubtitle.text = row.subtitle
        }
    }

    private class NoteHolder(
        private val views: ItemSystemNoteBinding,
        callback: SystemAdapterCallback,
    ) : SystemHolder(views, callback) {
        override fun bind(item: SystemRow) {
            views.systemNoteText.text = (item as SystemRow.Note).text
        }
    }

    private companion object {
        const val TYPE_STATUS = 0
        const val TYPE_CARD = 1
        const val TYPE_GROUP = 2
        const val TYPE_ITEM = 3
        const val TYPE_NOTE = 4
    }
}
