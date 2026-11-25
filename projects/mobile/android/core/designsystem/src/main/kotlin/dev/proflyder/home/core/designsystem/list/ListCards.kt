package dev.proflyder.home.core.designsystem.list

import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Rect
import android.view.View
import androidx.annotation.Px
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import com.google.android.material.shape.MaterialShapeDrawable
import dev.proflyder.home.core.designsystem.R

private val CARD_SLOT = GroupSlot(first = true, last = true)

class ListCards(
    @Px private val radius: Float,
    @Px private val gap: Int,
) : RecyclerView.ItemDecoration() {
    private val background = MaterialShapeDrawable(CARD_SLOT.shape(radius))

    override fun getItemOffsets(outRect: Rect, view: View, parent: RecyclerView, state: RecyclerView.State) {
        if (!parent.isCard(view)) return
        outRect.bottom = gap
        view.groupedRowRipple(radius).bind(CARD_SLOT)
    }

    override fun onDraw(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        background.fillColor = ColorStateList.valueOf(MaterialColors.getColor(parent, com.google.android.material.R.attr.colorSurfaceContainer))
        parent.children().filter(parent::isCard).forEach { drawCard(canvas, it) }
    }

    private fun drawCard(canvas: Canvas, child: View) {
        val dx = child.translationX.toInt()
        val dy = child.translationY.toInt()
        background.setBounds(child.left + dx, child.top + dy, child.right + dx, child.bottom + dy)
        background.alpha = (child.alpha * OPAQUE).toInt()
        background.draw(canvas)
    }

    private companion object {
        const val OPAQUE = 255
    }
}

private fun RecyclerView.isCard(child: View): Boolean = slotOfChild(child) != null

fun RecyclerView.addListCards() {
    addItemDecoration(ListCards(resources.getDimension(R.dimen.ds_radius_l), resources.getDimensionPixelSize(R.dimen.ds_space_s)))
}
