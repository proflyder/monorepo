package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import com.google.android.material.chip.Chip
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFilterBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFiltersBinding

internal class FiltersHolder(
    private val views: ItemControlFiltersBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    override fun bind(item: ControlItem) {
        val filters = item as ControlItem.Filters
        views.controlFilters.bindChildren(filters.chips, ::createChip) { binder, chip ->
            binder.bind(chip, selected = chip.filter == filters.selected)
        }
    }

    private fun createChip(parent: ViewGroup): Pair<View, FilterChipBinder> {
        val chip = ItemControlFilterBinding.inflate(LayoutInflater.from(parent.context), parent, false).root
        return chip to FilterChipBinder(chip, callback.dispatch)
    }
}

internal class FilterChipBinder(
    private val chip: Chip,
    private val dispatch: (ControlIntent) -> Unit,
) {
    private var filter: ControlFilter? = null
    private val checked = SwitchBinder(chip) { wanted -> select(wanted) }

    fun bind(item: ControlItem.FilterChip, selected: Boolean) {
        filter = item.filter
        chip.text = item.label
        checked.render(selected)
    }

    private fun select(wanted: Boolean) {
        val target = filter?.takeIf { wanted } ?: return
        dispatch(ControlIntent.SelectFilter(target))
    }
}
