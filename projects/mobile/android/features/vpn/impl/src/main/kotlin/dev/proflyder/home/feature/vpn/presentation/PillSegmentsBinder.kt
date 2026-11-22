package dev.proflyder.home.feature.vpn.presentation

import dev.proflyder.home.core.designsystem.PillSegments
import dev.proflyder.home.core.presentation.ControlBinder

internal class PillSegmentsBinder<T>(
    private val pills: PillSegments,
    private val values: List<T>,
    private val onIntent: (T) -> Unit,
) : ControlBinder<T> {
    private var rendered: Int = pills.selectedIndex

    init {
        pills.onSelected = ::onTapped
    }

    override fun render(value: T) {
        rendered = values.indexOf(value)
        if (pills.selectedIndex != rendered) pills.select(rendered)
    }

    private fun onTapped(index: Int) {
        pills.select(rendered)
        values.getOrNull(index)?.let(onIntent)
    }
}
