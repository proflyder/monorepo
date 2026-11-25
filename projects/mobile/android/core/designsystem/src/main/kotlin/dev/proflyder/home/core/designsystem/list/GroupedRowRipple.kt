package dev.proflyder.home.core.designsystem.list

import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.RippleDrawable
import android.view.View
import androidx.annotation.Px
import androidx.core.content.ContextCompat
import com.google.android.material.color.MaterialColors
import com.google.android.material.shape.MaterialShapeDrawable
import com.google.android.material.shape.ShapeAppearanceModel
import dev.proflyder.home.core.designsystem.R

class GroupedRowRipple(
    private val view: View,
    @Px private val radius: Float,
) {
    private val color: ColorStateList = MaterialColors.getColorStateList(
        view.context,
        R.attr.homeRowItemRippleColor,
        checkNotNull(ContextCompat.getColorStateList(view.context, R.color.ds_row_item_ripple)),
    )

    private val ripples = mutableMapOf<GroupSlot, RippleDrawable>()

    var slot: GroupSlot? = null
        private set

    val mask: MaterialShapeDrawable?
        get() = slot?.let { ripples[it] }?.findDrawableByLayerId(android.R.id.mask) as? MaterialShapeDrawable

    fun bind(slot: GroupSlot) {
        this.slot = slot
        view.foreground = ripples.getOrPut(slot) { rippleFor(slot) }.takeIf { view.isClickable }
    }

    private fun rippleFor(slot: GroupSlot): RippleDrawable {
        val mask = MaterialShapeDrawable(slot.shape(radius)).apply { fillColor = ColorStateList.valueOf(Color.WHITE) }
        return RippleDrawable(color, null, mask)
    }
}

fun GroupSlot.shape(@Px radius: Float): ShapeAppearanceModel {
    val top = if (first) radius else 0f
    val bottom = if (last) radius else 0f
    return ShapeAppearanceModel.builder()
        .setTopLeftCornerSize(top)
        .setTopRightCornerSize(top)
        .setBottomLeftCornerSize(bottom)
        .setBottomRightCornerSize(bottom)
        .build()
}
