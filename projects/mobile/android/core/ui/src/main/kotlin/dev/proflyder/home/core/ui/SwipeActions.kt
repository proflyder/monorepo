package dev.proflyder.home.core.ui

import android.content.Context
import android.graphics.Canvas
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Keyed

class SwipeActions<L, R>(
    context: Context,
    private val left: SwipeAction<L>,
    private val right: SwipeAction<R>,
    private val itemAt: (Int) -> Keyed?,
    private val dispatchLeft: (L) -> Unit,
    private val dispatchRight: (R) -> Unit,
) : ItemTouchHelper.SimpleCallback(0, ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT) {
    private val background = SwipeBackground(context)

    override fun onMove(
        recyclerView: RecyclerView,
        viewHolder: RecyclerView.ViewHolder,
        target: RecyclerView.ViewHolder,
    ): Boolean = false

    override fun getSwipeDirs(recyclerView: RecyclerView, viewHolder: RecyclerView.ViewHolder): Int {
        val position = viewHolder.bindingAdapterPosition
        if (itemAt(position) == null) return 0
        val leftDirs = if (left.intentFor(position) != null) ItemTouchHelper.LEFT else 0
        val rightDirs = if (right.intentFor(position) != null) ItemTouchHelper.RIGHT else 0
        return leftDirs or rightDirs
    }

    override fun getSwipeThreshold(viewHolder: RecyclerView.ViewHolder): Float = SWIPE_THRESHOLD

    override fun onSwiped(viewHolder: RecyclerView.ViewHolder, direction: Int) {
        val position = viewHolder.bindingAdapterPosition
        if (direction == ItemTouchHelper.LEFT) {
            left.intentFor(position)?.let(dispatchLeft)
        } else {
            right.intentFor(position)?.let(dispatchRight)
        }
        recover(viewHolder)
    }

    override fun clearView(recyclerView: RecyclerView, viewHolder: RecyclerView.ViewHolder) {
        super.clearView(recyclerView, viewHolder)
        reset(viewHolder)
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
        if (!isCurrentlyActive) {
            reset(viewHolder)
            return
        }
        val action = if (dX < 0) left else right
        background.draw(canvas, viewHolder.itemView, dX, action)
        super.onChildDraw(canvas, recyclerView, viewHolder, dX, dY, actionState, isCurrentlyActive)
    }

    private fun recover(viewHolder: RecyclerView.ViewHolder) {
        val recyclerView = viewHolder.itemView.parent as? RecyclerView ?: return
        clearView(recyclerView, viewHolder)
        reset(viewHolder)
    }

    private fun reset(viewHolder: RecyclerView.ViewHolder) {
        viewHolder.itemView.translationX = 0f
        viewHolder.itemView.translationY = 0f
    }

    private companion object {
        const val SWIPE_THRESHOLD = 0.35f
    }
}
