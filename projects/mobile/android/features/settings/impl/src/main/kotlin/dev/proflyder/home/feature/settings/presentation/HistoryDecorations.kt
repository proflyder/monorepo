package dev.proflyder.home.feature.settings.presentation

import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.addGroupedRows
import dev.proflyder.home.feature.settings.impl.R

internal fun RecyclerView.addHistoryDecorations() {
    addGroupedRows(resources.getDimensionPixelSize(R.dimen.settings_history_divider_inset))
}
