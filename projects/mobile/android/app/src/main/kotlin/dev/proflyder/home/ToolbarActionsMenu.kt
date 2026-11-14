package dev.proflyder.home

import android.content.Context
import android.graphics.Color
import android.view.Gravity
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.PopupWindow
import androidx.core.graphics.drawable.toDrawable
import dev.proflyder.home.databinding.AppToolbarMenuBinding

/**
 * The toolbar's single actions button opens this custom menu instead of a
 * framework overflow list: a rounded tonal card with one icon and label per
 * row, animated out of the button and dismissed by a touch outside.
 *
 * Row ids double as action ids, so the host maps a row to its behaviour with a
 * single `when`.
 */
internal class ToolbarActionsMenu(
    private val context: Context,
    private val onAction: (Int) -> Unit,
) {
    private var popup: PopupWindow? = null

    /** The open menu card, or null while it is closed; tests inspect the rows. */
    internal val contentView: View?
        get() = popup?.contentView

    fun toggle(anchor: View) {
        when (popup?.isShowing) {
            true -> popup?.dismiss()
            else -> show(anchor)
        }
    }

    fun dismiss() {
        popup?.dismiss()
    }

    private fun show(anchor: View) {
        val binding = AppToolbarMenuBinding.inflate(LayoutInflater.from(context))
        binding.menuActionCall.setOnClickListener { select(binding.menuActionCall.id) }
        binding.menuActionSettings.setOnClickListener { select(binding.menuActionSettings.id) }
        popup = build(binding.root).also { it.showAsDropDown(anchor, 0, 0, Gravity.END) }
    }

    private fun select(actionId: Int) {
        dismiss()
        onAction(actionId)
    }

    private fun build(content: View): PopupWindow {
        val window = PopupWindow(
            content,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            true,
        )
        window.animationStyle = R.style.Animation_Home_ToolbarMenu
        window.elevation = context.resources.getDimension(R.dimen.ui_popup_elevation)
        window.setBackgroundDrawable(Color.TRANSPARENT.toDrawable())
        window.isOutsideTouchable = true
        return window
    }
}
