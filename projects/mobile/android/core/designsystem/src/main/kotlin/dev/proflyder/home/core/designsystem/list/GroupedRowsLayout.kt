package dev.proflyder.home.core.designsystem.list

import android.content.Context
import android.util.AttributeSet
import android.widget.LinearLayout
import androidx.core.view.children
import androidx.core.view.isVisible
import com.google.android.material.card.MaterialCardView
import dev.proflyder.home.core.designsystem.R

class GroupedRowsLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    init {
        orientation = VERTICAL
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        super.onLayout(changed, l, t, r, b)
        bindRipples()
    }

    private fun bindRipples() {
        val rows = children.filter { it.isVisible }.toList()
        val radius = cornerRadius()
        rows.forEachIndexed { index, row ->
            row.groupedRowRipple(radius).bind(GroupSlot(first = index == 0, last = index == rows.lastIndex))
        }
    }

    private fun cornerRadius(): Float =
        (parent as? MaterialCardView)?.radius ?: resources.getDimension(R.dimen.ds_radius_l)
}
