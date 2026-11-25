package dev.proflyder.home.core.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View
import androidx.annotation.AttrRes
import androidx.annotation.DrawableRes
import androidx.core.content.ContextCompat
import androidx.core.graphics.withClip
import com.google.android.material.color.MaterialColors

class SwipeBackground(private val context: Context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val radius = context.resources.getDimension(dev.proflyder.home.core.designsystem.R.dimen.ds_radius_l)
    private val inset = context.resources.getDimensionPixelSize(R.dimen.ui_swipe_icon_inset)

    fun draw(canvas: Canvas, item: View, dx: Float, action: SwipeAction) {
        if (dx == 0f) return
        val left = dx < 0
        val area = RectF(item.left.toFloat(), item.top.toFloat(), item.right.toFloat(), item.bottom.toFloat())
        paint.color = MaterialColors.getColor(context, action.style.colorAttr, 0)
        canvas.drawRoundRect(area, radius, radius, paint)
        val icon = icon(action.style)
        canvas.withClip(area) {
            placeIcon(icon, item, left)
            icon.draw(this)
        }
    }

    private fun icon(style: SwipeStyle): Drawable =
        checkNotNull(ContextCompat.getDrawable(context, style.icon)).mutate().also {
            it.setTint(MaterialColors.getColor(context, style.tintAttr, 0))
        }

    private fun placeIcon(icon: Drawable, item: View, left: Boolean) {
        val size = icon.intrinsicWidth
        val top = item.top + (item.height - size) / 2
        val start = if (left) item.right - inset - size else item.left + inset
        icon.setBounds(start, top, start + size, top + size)
    }
}

enum class SwipeStyle(
    @DrawableRes val icon: Int,
    @AttrRes val colorAttr: Int,
    @AttrRes val tintAttr: Int,
) {
    Delete(R.drawable.ui_ic_delete, com.google.android.material.R.attr.colorError, com.google.android.material.R.attr.colorOnError),
    ToggleRead(R.drawable.ui_ic_mail, androidx.appcompat.R.attr.colorPrimary, com.google.android.material.R.attr.colorOnPrimary),
}

data class SwipeAction(
    val direction: Int,
    val style: SwipeStyle,
    val onSwiped: (position: Int) -> Unit,
)

fun swipeAction(direction: Int, style: SwipeStyle, onSwiped: (position: Int) -> Unit): SwipeAction =
    SwipeAction(direction, style, onSwiped)
