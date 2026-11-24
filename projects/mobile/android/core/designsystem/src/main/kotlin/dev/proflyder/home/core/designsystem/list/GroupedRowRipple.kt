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

    private val ripples = mutableMapOf<GroupSlot, RippleMask>()

    var topRadius: Float = 0f
        private set

    var bottomRadius: Float = 0f
        private set

    var mask: MaterialShapeDrawable? = null
        private set

    fun bind(slot: GroupSlot?, enabled: Boolean) {
        topRadius = if (slot?.first == true) radius else 0f
        bottomRadius = if (slot?.last == true) radius else 0f
        val shape = slot ?: NO_SLOT
        val current = ripples.getOrPut(shape) { RippleMask(shape) }
        mask = current.mask
        view.foreground = current.drawable.takeIf { enabled }
        view.isClickable = enabled
        view.isFocusable = enabled
    }

    private inner class RippleMask(slot: GroupSlot) {
        val mask: MaterialShapeDrawable = MaterialShapeDrawable(
            ShapeAppearanceModel.builder()
                .setTopLeftCornerSize(if (slot.first) radius else 0f)
                .setTopRightCornerSize(if (slot.first) radius else 0f)
                .setBottomLeftCornerSize(if (slot.last) radius else 0f)
                .setBottomRightCornerSize(if (slot.last) radius else 0f)
                .build(),
        ).apply { fillColor = ColorStateList.valueOf(Color.WHITE) }

        val drawable = RippleDrawable(color, null, mask)
    }

    private companion object {
        val NO_SLOT = GroupSlot(first = false, last = false)
    }
}
