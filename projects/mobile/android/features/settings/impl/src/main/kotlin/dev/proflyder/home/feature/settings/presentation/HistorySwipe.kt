package dev.proflyder.home.feature.settings.presentation

import android.graphics.Canvas
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.feature.push.NotificationHistoryEntry

internal interface HistorySwipeActions {
    fun entryAt(position: Int): NotificationHistoryEntry?

    fun onDelete(entry: NotificationHistoryEntry)

    fun onToggleRead(entry: NotificationHistoryEntry)
}

internal class HistorySwipe(
    private val actions: HistorySwipeActions,
    private val background: HistorySwipeBackground,
) : ItemTouchHelper.SimpleCallback(0, ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT) {
    override fun onMove(
        recyclerView: RecyclerView,
        viewHolder: RecyclerView.ViewHolder,
        target: RecyclerView.ViewHolder,
    ): Boolean = false

    override fun getSwipeDirs(recyclerView: RecyclerView, viewHolder: RecyclerView.ViewHolder): Int =
        actions.entryAt(viewHolder.bindingAdapterPosition)?.let { ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT } ?: 0

    override fun getSwipeThreshold(viewHolder: RecyclerView.ViewHolder): Float = SWIPE_THRESHOLD

    override fun onSwiped(viewHolder: RecyclerView.ViewHolder, direction: Int) {
        val position = viewHolder.bindingAdapterPosition
        val entry = actions.entryAt(position) ?: return
        when (direction) {
            ItemTouchHelper.LEFT -> actions.onDelete(entry)
            else -> toggle(viewHolder, position, entry)
        }
    }

    override fun onChildDraw(
        canvas: Canvas,
        recyclerView: RecyclerView,
        viewHolder: RecyclerView.ViewHolder,
        dX: Float,
        dY: Float,
        actionState: Int,
        isCurrentlyActive: Boolean,
    ) {
        val entry = actions.entryAt(viewHolder.bindingAdapterPosition)
        if (entry != null) background.draw(canvas, viewHolder.itemView, dX, entry.read)
        super.onChildDraw(canvas, recyclerView, viewHolder, dX, dY, actionState, isCurrentlyActive)
    }

    private fun toggle(viewHolder: RecyclerView.ViewHolder, position: Int, entry: NotificationHistoryEntry) {
        actions.onToggleRead(entry)
        viewHolder.bindingAdapter?.notifyItemChanged(position)
    }

    private companion object {
        const val SWIPE_THRESHOLD = 0.35f
    }
}
