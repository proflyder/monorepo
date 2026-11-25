package dev.proflyder.home.feature.settings.presentation

import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.addFullWidthRows
import dev.proflyder.home.feature.settings.impl.R

internal fun RecyclerView.addHistoryDecorations() {
    addFullWidthRows(resources.getDimensionPixelSize(R.dimen.settings_history_divider_inset))
}
