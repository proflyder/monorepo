package dev.proflyder.home.core.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View
import androidx.annotation.ColorInt
import androidx.annotation.AttrRes
import androidx.annotation.DrawableRes
import androidx.core.content.ContextCompat
import androidx.core.graphics.withClip
import kotlin.math.abs

class SwipeBackground(private val context: Context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val radius = context.resources.getDimension(R.dimen.ui_swipe_radius)
    private val inset = context.resources.getDimensionPixelSize(R.dimen.ui_swipe_icon_inset)

    fun draw(canvas: Canvas, item: View, dx: Float, action: SwipeAction<*>) {
        if (dx == 0f) return
        val left = dx < 0
        val area = area(item, dx)
        paint.color = action.color
        canvas.drawRoundRect(area, radius, radius, paint)
        val icon = icon(action)
        canvas.withClip(area) {
            placeIcon(icon, item, left)
            icon.draw(this)
        }
    }

    private fun icon(action: SwipeAction<*>): Drawable =
        checkNotNull(ContextCompat.getDrawable(context, action.icon)).mutate().also { it.setTint(action.iconTint) }

    private fun area(item: View, dx: Float): RectF = when {
        dx < 0 -> RectF(item.right + dx, item.top.toFloat(), item.right.toFloat(), item.bottom.toFloat())
        else -> RectF(item.left.toFloat(), item.top.toFloat(), item.left + abs(dx), item.bottom.toFloat())
    }

    private fun placeIcon(icon: Drawable, item: View, left: Boolean) {
        val size = icon.intrinsicWidth
        val top = item.top + (item.height - size) / 2
        val start = if (left) item.right - inset - size else item.left + inset
        icon.setBounds(start, top, start + size, top + size)
    }
}

data class SwipeAction<I>(
    val direction: Int,
    @DrawableRes val icon: Int,
    @ColorInt val color: Int,
    @ColorInt val iconTint: Int,
    val intentFor: (Int) -> I?,
)

fun <I> swipeAction(
    context: Context,
    direction: Int,
    @DrawableRes icon: Int,
    @AttrRes colorAttr: Int,
    @AttrRes tintAttr: Int,
    intentFor: (Int) -> I?,
): SwipeAction<I> = SwipeAction(
    direction,
    icon,
    com.google.android.material.color.MaterialColors.getColor(context, colorAttr, 0),
    com.google.android.material.color.MaterialColors.getColor(context, tintAttr, 0),
    intentFor,
)
