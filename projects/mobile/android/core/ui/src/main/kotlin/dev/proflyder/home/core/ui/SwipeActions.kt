package dev.proflyder.home.core.ui

import android.content.Context
import android.graphics.Canvas
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Keyed

class SwipeActions<I>(
    context: Context,
    private val left: SwipeAction<I>,
    private val right: SwipeAction<I>,
    private val itemAt: (Int) -> Keyed?,
    private val dispatch: (I) -> Unit,
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
        return actions().filter { it.intentFor(position) != null }.fold(0) { dirs, action -> dirs or action.direction }
    }

    override fun getSwipeThreshold(viewHolder: RecyclerView.ViewHolder): Float = SWIPE_THRESHOLD

    override fun onSwiped(viewHolder: RecyclerView.ViewHolder, direction: Int) {
        val position = viewHolder.bindingAdapterPosition
        val action = actionFor(direction) ?: return
        val intent = action.intentFor(position) ?: return
        dispatch(intent)
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
        val action = actionFor(dX) ?: return
        background.draw(canvas, viewHolder.itemView, dX, action)
        super.onChildDraw(canvas, recyclerView, viewHolder, dX, dY, actionState, isCurrentlyActive)
    }

    private fun actions(): List<SwipeAction<I>> = listOf(left, right)

    private fun actionFor(direction: Int): SwipeAction<I>? = actions().firstOrNull { it.direction == direction }

    private fun actionFor(dx: Float): SwipeAction<I>? = actionFor(if (dx < 0) ItemTouchHelper.LEFT else ItemTouchHelper.RIGHT)

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
