package dev.proflyder.home.core.designsystem.list

import android.graphics.Rect
import android.view.View
import androidx.annotation.Px
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.R

class GroupRowRipples(@Px private val radius: Float) : RecyclerView.ItemDecoration() {
    override fun getItemOffsets(outRect: Rect, view: View, parent: RecyclerView, state: RecyclerView.State) {
        parent.slotOfChild(view)?.let { rippleOf(view).bind(it) }
    }

    private fun rippleOf(view: View): GroupedRowRipple =
        view.getTag(R.id.ds_grouped_row_ripple) as? GroupedRowRipple
            ?: GroupedRowRipple(view, radius).also { view.setTag(R.id.ds_grouped_row_ripple, it) }
}

fun RecyclerView.addGroupedRows(@Px dividerInset: Int = 0) {
    val radius = resources.getDimension(R.dimen.ds_radius_l)
    addItemDecoration(GroupCards(radius, resources.getDimensionPixelSize(R.dimen.ds_space_m)))
    addItemDecoration(GroupDividers(dividerInset))
    addItemDecoration(GroupRowRipples(radius))
}
