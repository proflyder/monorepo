package dev.proflyder.home.core.ui

import android.view.View
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.DrawableCompat
import com.google.android.material.color.MaterialColors
import com.google.android.material.shape.MaterialShapeDrawable
import com.google.android.material.shape.ShapeAppearanceModel
import com.google.android.material.snackbar.BaseTransientBottomBar
import com.google.android.material.snackbar.Snackbar

/** How a [Notices] message is tinted: neutral confirmation or a problem. */
enum class NoticeKind { Info, Error }

/**
 * The single way the app shows transient feedback: a rounded tonal pill with a status icon,
 * floating above the view tagged [R.id.home_notice_anchor] so it never covers navigation.
 */
object Notices {
    fun show(view: View, message: CharSequence, kind: NoticeKind = NoticeKind.Info, long: Boolean = false) {
        val context = view.context
        val snackbar = Snackbar.make(
            view,
            message,
            if (long) Snackbar.LENGTH_LONG else Snackbar.LENGTH_SHORT,
        )
        val accent = MaterialColors.getColor(
            view,
            if (kind == NoticeKind.Error) androidx.appcompat.R.attr.colorError else androidx.appcompat.R.attr.colorPrimary,
        )
        val container = snackbar.view
        val radius = context.resources.displayMetrics.density * 20
        container.background = MaterialShapeDrawable(
            ShapeAppearanceModel.builder().setAllCornerSizes(radius).build(),
        ).apply {
            fillColor = android.content.res.ColorStateList.valueOf(
                MaterialColors.getColor(view, com.google.android.material.R.attr.colorSurfaceContainerHighest),
            )
            setStroke(context.resources.displayMetrics.density, accent)
            elevation = context.resources.displayMetrics.density * 6
        }
        container.backgroundTintList = null
        val text = container.findViewById<TextView>(com.google.android.material.R.id.snackbar_text)
        text.setTextColor(MaterialColors.getColor(view, com.google.android.material.R.attr.colorOnSurface))
        text.maxLines = 3
        text.setCompoundDrawablesRelativeWithIntrinsicBounds(icon(kind, accent, view), null, null, null)
        text.compoundDrawablePadding = (context.resources.displayMetrics.density * 12).toInt()
        view.rootView.findViewById<View>(R.id.home_notice_anchor)?.let { snackbar.anchorView = it }
        snackbar.animationMode = BaseTransientBottomBar.ANIMATION_MODE_SLIDE
        snackbar.show()
    }

    private fun icon(kind: NoticeKind, accent: Int, view: View) = ContextCompat.getDrawable(
        view.context,
        if (kind == NoticeKind.Error) R.drawable.ui_ic_notice_error else R.drawable.ui_ic_notice_success,
    )?.mutate()?.also { DrawableCompat.setTint(it, accent) }
}
