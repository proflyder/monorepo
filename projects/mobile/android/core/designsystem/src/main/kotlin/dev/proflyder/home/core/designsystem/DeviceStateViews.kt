package dev.proflyder.home.core.designsystem

import android.animation.ValueAnimator
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.view.View
import android.widget.ImageView
import android.widget.TextView
import androidx.annotation.ColorInt
import com.google.android.material.card.MaterialCardView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.ColorRole

object DeviceStateViews {
    fun bindIcon(icon: ImageView, on: Boolean) {
        val fill = fillColor(icon, on)
        val content = contentColor(icon, on)
        icon.isActivated = on
        val from = icon.getTag(R.id.ds_device_state_fill) as? Int
        icon.setTag(R.id.ds_device_state_fill, fill)
        when {
            from == null || from == fill || !icon.isAttachedToWindow -> paintIcon(icon, fill, content)
            else -> animateIcon(icon, from, fill, content)
        }
    }

    fun bindStatus(status: TextView, on: Boolean) {
        status.setTextColor(statusColor(status, on))
        status.setTypeface(Typeface.create(status.typeface, Typeface.NORMAL), statusStyle(on))
    }

    fun bindCard(card: MaterialCardView, on: Boolean) {
        card.setStrokeColor(ColorStateList.valueOf(resolve(card, ColorRole.STATE_ON)))
        card.strokeWidth = when {
            on -> card.resources.getDimensionPixelSize(R.dimen.ds_stroke_thin)
            else -> 0
        }
    }

    fun clearIcon(icon: ImageView) {
        icon.setTag(R.id.ds_device_state_fill, null)
        icon.backgroundTintList = null
    }

    @ColorInt
    fun fillColor(view: View, on: Boolean): Int = resolve(view, fillRole(on))

    @ColorInt
    fun contentColor(view: View, on: Boolean): Int = resolve(view, contentRole(on))

    @ColorInt
    fun statusColor(view: View, on: Boolean): Int = resolve(view, statusRole(on))

    fun fillRole(on: Boolean): ColorRole = when {
        on -> ColorRole.STATE_ON
        else -> ColorRole.SURFACE_CONTAINER_HIGHEST
    }

    fun contentRole(on: Boolean): ColorRole = when {
        on -> ColorRole.ON_STATE_ON
        else -> ColorRole.ON_SURFACE_VARIANT
    }

    fun statusRole(on: Boolean): ColorRole = when {
        on -> ColorRole.STATE_ON_TEXT
        else -> ColorRole.ON_SURFACE_VARIANT
    }

    @ColorInt
    private fun resolve(view: View, role: ColorRole): Int =
        MaterialColors.getColor(view.context, role.attr, MaterialColors.getColor(view, fallback(role).attr))

    private fun fallback(role: ColorRole): ColorRole = when (role) {
        ColorRole.STATE_ON, ColorRole.STATE_ON_TEXT -> ColorRole.PRIMARY
        ColorRole.ON_STATE_ON -> ColorRole.ON_PRIMARY
        else -> role
    }

    private fun statusStyle(on: Boolean): Int = when {
        on -> Typeface.BOLD
        else -> Typeface.NORMAL
    }

    private fun paintIcon(icon: ImageView, @ColorInt fill: Int, @ColorInt content: Int) {
        icon.backgroundTintList = ColorStateList.valueOf(fill)
        icon.setColorFilter(content)
    }

    private fun animateIcon(icon: ImageView, @ColorInt from: Int, @ColorInt fill: Int, @ColorInt content: Int) {
        icon.setColorFilter(content)
        val animator = ValueAnimator.ofArgb(from, fill)
        animator.duration = HomeMotion.selectionDuration(icon.context).toLong()
        animator.addUpdateListener { icon.backgroundTintList = ColorStateList.valueOf(it.animatedValue as Int) }
        animator.start()
    }
}
