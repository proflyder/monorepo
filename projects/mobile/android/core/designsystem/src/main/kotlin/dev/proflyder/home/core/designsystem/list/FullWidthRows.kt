package dev.proflyder.home.core.designsystem.list

import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.drawable.Drawable
import android.graphics.drawable.RippleDrawable
import android.view.View
import androidx.annotation.Px
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.toDrawable
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.R

class FullWidthRows(@Px private val dividerInset: Int) : RecyclerView.ItemDecoration() {
    private val divider = Paint(Paint.ANTI_ALIAS_FLAG)

    override fun getItemOffsets(outRect: Rect, view: View, parent: RecyclerView, state: RecyclerView.State) {
        if (parent.slotOfChild(view) == null) return
        view.foreground = view.fullRowRipple().takeIf { view.isClickable }
    }

    override fun onDraw(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        divider.color = MaterialColors.getColor(parent, com.google.android.material.R.attr.colorOutlineVariant)
        val thickness = parent.resources.displayMetrics.density.coerceAtLeast(1f)
        parent.children().forEach { child -> parent.slotOfChild(child)?.let { drawRow(canvas, child, it, thickness) } }
    }

    private fun drawRow(canvas: Canvas, child: View, slot: GroupSlot, thickness: Float) {
        if (slot.last) return
        val bottom = child.bottom + child.translationY
        canvas.drawRect(child.left + child.translationX + dividerInset, bottom - thickness, child.right + child.translationX, bottom, divider)
    }
}

private fun View.fullRowRipple(): Drawable =
    getTag(R.id.ds_full_row_ripple) as? Drawable ?: newRipple().also { setTag(R.id.ds_full_row_ripple, it) }

private fun View.newRipple(): Drawable {
    val fallback = checkNotNull(ContextCompat.getColorStateList(context, R.color.ds_row_item_ripple))
    val color = MaterialColors.getColorStateList(context, R.attr.homeRowItemRippleColor, fallback)
    return RippleDrawable(color, null, Color.WHITE.toDrawable())
}

fun RecyclerView.addFullWidthRows(@Px dividerInset: Int = resources.getDimensionPixelSize(R.dimen.ds_space_l)) {
    addItemDecoration(FullWidthRows(dividerInset))
}
