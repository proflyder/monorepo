package dev.proflyder.home.feature.settings.presentation

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View
import androidx.annotation.AttrRes
import androidx.annotation.DrawableRes
import androidx.core.content.ContextCompat
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.feature.settings.impl.R
import kotlin.math.abs

internal class HistorySwipeBackground(private val context: Context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val radius = context.resources.getDimension(R.dimen.settings_history_swipe_radius)
    private val inset = context.resources.getDimensionPixelSize(R.dimen.settings_history_swipe_icon_inset)
    private val delete = side(R.drawable.settings_ic_delete, com.google.android.material.R.attr.colorError, com.google.android.material.R.attr.colorOnError)
    private val markRead = side(R.drawable.settings_ic_mark_read, androidx.appcompat.R.attr.colorPrimary, com.google.android.material.R.attr.colorOnPrimary)
    private val markUnread = side(R.drawable.settings_ic_mark_unread, androidx.appcompat.R.attr.colorPrimary, com.google.android.material.R.attr.colorOnPrimary)

    fun draw(canvas: Canvas, item: View, dx: Float, read: Boolean) {
        if (dx == 0f) return
        val left = dx < 0
        val area = area(item, dx)
        val side = sideFor(left, read)
        paint.color = side.color
        canvas.drawRoundRect(area, radius, radius, paint)
        canvas.save()
        canvas.clipRect(area)
        placeIcon(side.icon, item, left)
        side.icon.draw(canvas)
        canvas.restore()
    }

    fun sideFor(left: Boolean, read: Boolean): Side = when {
        left -> delete
        read -> markUnread
        else -> markRead
    }

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

    private fun side(@DrawableRes icon: Int, @AttrRes background: Int, @AttrRes foreground: Int): Side {
        val drawable = checkNotNull(ContextCompat.getDrawable(context, icon)).mutate()
        drawable.setTint(MaterialColors.getColor(context, foreground, 0))
        return Side(MaterialColors.getColor(context, background, 0), drawable, icon)
    }

    internal class Side(val color: Int, val icon: Drawable, @DrawableRes val iconRes: Int)
}
