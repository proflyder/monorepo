package dev.proflyder.home.core.designsystem.list

import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.RecyclerView

interface SectionedAdapter {
    fun isSectionHeader(position: Int): Boolean

    fun isStickyHeader(position: Int): Boolean = isSectionHeader(position)
}

data class GroupSlot(val first: Boolean, val last: Boolean)

fun SectionedAdapter.slotOf(position: Int, count: Int): GroupSlot? {
    if (position !in 0 until count || isSectionHeader(position)) return null
    return GroupSlot(
        first = position == 0 || isSectionHeader(position - 1),
        last = position == count - 1 || isSectionHeader(position + 1),
    )
}

fun SectionedAdapter.stickyHeaderAtOrBefore(position: Int): Int? =
    (position downTo 0).firstOrNull(::isStickyHeader)

internal fun RecyclerView.sectionedAdapter(): Pair<RecyclerView.Adapter<*>, SectionedAdapter>? {
    val current = adapter ?: return null
    return current.sections()?.let { current to it }
}

fun RecyclerView.Adapter<*>.sections(): SectionedAdapter? = when (this) {
    is SectionedAdapter -> this
    is ConcatAdapter -> ConcatSections(this).takeIf { adapters.any { it.sections() != null } }
    else -> null
}

private class ConcatSections(private val concat: ConcatAdapter) : SectionedAdapter {
    override fun isSectionHeader(position: Int): Boolean = resolve(position, outside = true, SectionedAdapter::isSectionHeader)

    override fun isStickyHeader(position: Int): Boolean = resolve(position, outside = false, SectionedAdapter::isStickyHeader)

    private fun resolve(position: Int, outside: Boolean, check: SectionedAdapter.(Int) -> Boolean): Boolean {
        if (position !in 0 until concat.itemCount) return outside
        val wrapped = concat.getWrappedAdapterAndPosition(position)
        val sections = wrapped.first.sections() ?: return outside
        return sections.check(wrapped.second)
    }
}

internal fun RecyclerView.slotOfChild(child: android.view.View): GroupSlot? {
    val (adapter, sections) = sectionedAdapter() ?: return null
    val position = getChildAdapterPosition(child)
    return sections.slotOf(position, adapter.itemCount)
}
