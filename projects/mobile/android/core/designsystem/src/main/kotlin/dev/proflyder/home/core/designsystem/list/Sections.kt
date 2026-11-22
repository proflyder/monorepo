package dev.proflyder.home.core.designsystem.list

import androidx.recyclerview.widget.RecyclerView

interface SectionedAdapter {
    fun isSectionHeader(position: Int): Boolean
}

data class GroupSlot(val first: Boolean, val last: Boolean)

fun SectionedAdapter.slotOf(position: Int, count: Int): GroupSlot? {
    if (position !in 0 until count || isSectionHeader(position)) return null
    return GroupSlot(
        first = position == 0 || isSectionHeader(position - 1),
        last = position == count - 1 || isSectionHeader(position + 1),
    )
}

fun SectionedAdapter.headerAtOrBefore(position: Int): Int? =
    (position downTo 0).firstOrNull(::isSectionHeader)

internal fun RecyclerView.sectionedAdapter(): Pair<RecyclerView.Adapter<*>, SectionedAdapter>? {
    val current = adapter ?: return null
    return (current as? SectionedAdapter)?.let { current to it }
}

internal fun RecyclerView.slotOfChild(child: android.view.View): GroupSlot? {
    val (adapter, sections) = sectionedAdapter() ?: return null
    val position = getChildAdapterPosition(child)
    return sections.slotOf(position, adapter.itemCount)
}
