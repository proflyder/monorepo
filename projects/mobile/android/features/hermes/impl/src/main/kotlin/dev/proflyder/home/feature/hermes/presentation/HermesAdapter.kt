package dev.proflyder.home.feature.hermes.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.applyFullRowRipple
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.feature.hermes.impl.databinding.ItemHermesBodyBinding
import dev.proflyder.home.feature.hermes.impl.databinding.ItemHermesEntryBinding
import dev.proflyder.home.feature.hermes.impl.databinding.ItemHermesHeaderBinding
import dev.proflyder.home.feature.hermes.impl.databinding.ItemHermesNoteBinding

private typealias HermesCallback = AdapterCallback<HermesRow>

private typealias HermesHolder = BaseViewHolder<HermesRow, ViewBinding, HermesCallback>

private val HermesDiff = KeyedDiff<HermesRow>(key = { it.key })

internal class HermesAdapter(
    callback: HermesCallback,
) : BaseListAdapter<HermesRow, ViewBinding, HermesCallback>(callback, HermesDiff),
    SectionedAdapter {
    override fun isSectionHeader(position: Int): Boolean = getItem(position) !is HermesRow.Entry

    override fun isStickyHeader(position: Int): Boolean = getItem(position) is HermesRow.Header

    override fun viewTypeOf(item: HermesRow): Int = when (item) {
        is HermesRow.Header -> TYPE_HEADER
        is HermesRow.Entry -> TYPE_ENTRY
        is HermesRow.Note -> TYPE_NOTE
        is HermesRow.Body -> TYPE_BODY
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): HermesHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemHermesHeaderBinding.inflate(inflater, parent, false), callback)
            TYPE_NOTE -> NoteHolder(ItemHermesNoteBinding.inflate(inflater, parent, false), callback)
            TYPE_BODY -> BodyHolder(ItemHermesBodyBinding.inflate(inflater, parent, false), callback)
            else -> EntryHolder(ItemHermesEntryBinding.inflate(inflater, parent, false), callback)
        }

    private class HeaderHolder(private val views: ItemHermesHeaderBinding, callback: HermesCallback) :
        HermesHolder(views, callback) {
        override fun bind(item: HermesRow) {
            views.hermesHeaderTitle.text = (item as HermesRow.Header).title
        }
    }

    private class EntryHolder(private val views: ItemHermesEntryBinding, callback: HermesCallback) :
        HermesHolder(views, callback) {
        override fun bind(item: HermesRow) {
            val entry = item as HermesRow.Entry
            val row = views.hermesEntryRow
            row.title = entry.title
            row.subtitle = entry.subtitle
            row.value = entry.value
            row.subtitleTone = entry.tone
            row.valueTone = entry.valueTone
            row.chevron = entry.target != null
            row.isClickable = entry.target != null
            row.setOnClickListener(entry.target?.let { { callback.onItemClick(entry) } })
            row.applyFullRowRipple()
        }
    }

    private class NoteHolder(private val views: ItemHermesNoteBinding, callback: HermesCallback) :
        HermesHolder(views, callback) {
        override fun bind(item: HermesRow) {
            views.hermesNoteText.text = (item as HermesRow.Note).text
        }
    }

    private class BodyHolder(private val views: ItemHermesBodyBinding, callback: HermesCallback) :
        HermesHolder(views, callback) {
        override fun bind(item: HermesRow) {
            views.hermesBodyText.text = (item as HermesRow.Body).text
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_ENTRY = 1
        const val TYPE_NOTE = 2
        const val TYPE_BODY = 3
    }
}
