package dev.proflyder.home.feature.settings.presentation

import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.designsystem.list.GroupCards
import dev.proflyder.home.core.designsystem.list.GroupDividers
import dev.proflyder.home.core.designsystem.list.StickySectionHeaders
import dev.proflyder.home.feature.settings.impl.R

internal fun RecyclerView.addHistoryDecorations() {
    val radius = resources.getDimension(DesignR.dimen.ds_radius_l)
    addItemDecoration(GroupCards(radius, resources.getDimensionPixelSize(DesignR.dimen.ds_space_m)))
    addItemDecoration(GroupDividers(resources.getDimensionPixelSize(R.dimen.settings_history_divider_inset)))
    StickySectionHeaders().attachTo(this)
}
