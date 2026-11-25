package dev.proflyder.home.core.designsystem.list

import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
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

    override fun onDraw(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        divider.color = MaterialColors.getColor(parent, com.google.android.material.R.attr.colorOutlineVariant)
        val thickness = parent.resources.getDimension(R.dimen.ds_full_row_divider).coerceAtLeast(1f)
        val rtl = parent.layoutDirection == View.LAYOUT_DIRECTION_RTL
        parent.children().filter(parent::hasRowBelow).forEach { drawDivider(canvas, it, thickness, rtl) }
    }

    private fun drawDivider(canvas: Canvas, child: View, thickness: Float, rtl: Boolean) {
        val shift = child.translationX
        val start = if (rtl) 0 else dividerInset
        val end = if (rtl) dividerInset else 0
        val bottom = child.bottom + child.translationY
        canvas.drawRect(child.left + shift + start, bottom - thickness, child.right + shift - end, bottom, divider)
    }
}

private fun RecyclerView.hasRowBelow(child: View): Boolean {
    if (sectionedAdapter() != null) return slotOfChild(child)?.last == false
    val holder = getChildViewHolder(child)
    val count = holder.bindingAdapter?.itemCount ?: return false
    return holder.bindingAdapterPosition in 0 until count - 1
}

fun View.applyFullRowRipple() {
    foreground = fullRowRipple().takeIf { isClickable }
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
