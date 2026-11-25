package dev.proflyder.home.feature.chat.presentation.sessions

import android.graphics.Canvas
import android.graphics.Paint
import android.view.View
import androidx.core.view.children
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.feature.chat.impl.R
import com.google.android.material.R as MaterialR

internal class SessionDividers(list: RecyclerView) : RecyclerView.ItemDecoration() {
    private val inset = list.resources.getDimension(R.dimen.chat_session_divider_inset)
    private val thickness = list.resources.getDimension(R.dimen.chat_session_divider_height)
    private val paint = Paint().apply { color = MaterialColors.getColor(list, MaterialR.attr.colorOutlineVariant) }

    override fun onDraw(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        val rtl = parent.layoutDirection == View.LAYOUT_DIRECTION_RTL
        val left = if (rtl) 0f else inset
        val right = parent.width - if (rtl) inset else 0f
        parent.children.filter { it.hasSessionBelow(parent) }.forEach { row ->
            val bottom = row.bottom + row.translationY
            canvas.drawRect(left, bottom - thickness, right, bottom, paint)
        }
    }

    private fun View.hasSessionBelow(parent: RecyclerView): Boolean {
        val holder = parent.getChildViewHolder(this)
        val adapter = holder.bindingAdapter as? ChatSessionsAdapter ?: return false
        val position = holder.bindingAdapterPosition
        return position != RecyclerView.NO_POSITION && position < adapter.itemCount - 1
    }
}
