package dev.proflyder.home.core.ui

import android.content.Context
import android.graphics.Canvas
import androidx.recyclerview.widget.ItemTouchHelper
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.HomeMotion
import dev.proflyder.home.core.model.Keyed

class SwipeActions(
    context: Context,
    private val left: SwipeAction,
    private val right: SwipeAction,
    private val itemAt: (Int) -> Keyed?,
) : ItemTouchHelper.SimpleCallback(0, ItemTouchHelper.LEFT or ItemTouchHelper.RIGHT) {
    private val background = SwipeBackground(context)
    private val helper = ItemTouchHelper(this)
    private var attached: RecyclerView? = null

    fun attachTo(list: RecyclerView) {
        attached = list
        helper.attachToRecyclerView(list)
    }

    override fun onMove(
        recyclerView: RecyclerView,
        viewHolder: RecyclerView.ViewHolder,
        target: RecyclerView.ViewHolder,
    ): Boolean = false

    override fun getSwipeDirs(recyclerView: RecyclerView, viewHolder: RecyclerView.ViewHolder): Int =
        if (itemAt(viewHolder.bindingAdapterPosition) == null) 0 else left.direction or right.direction

    override fun getSwipeThreshold(viewHolder: RecyclerView.ViewHolder): Float = SWIPE_THRESHOLD

    override fun getAnimationDuration(recyclerView: RecyclerView, animationType: Int, animateDx: Float, animateDy: Float): Long =
        when (animationType) {
            ItemTouchHelper.ANIMATION_TYPE_SWIPE_SUCCESS -> HomeMotion.exitDuration(recyclerView.context)
            else -> HomeMotion.enterDuration(recyclerView.context)
        }.toLong()

    override fun onSwiped(viewHolder: RecyclerView.ViewHolder, direction: Int) {
        val position = viewHolder.bindingAdapterPosition
        val action = if (direction == ItemTouchHelper.LEFT) left else right
        action.onSwiped(position)
        if (!action.style.removesRow) settleInPlace(viewHolder)
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
        if (dX != 0f) background.draw(canvas, viewHolder.itemView, dX, if (dX < 0) left else right)
        super.onChildDraw(canvas, recyclerView, viewHolder, dX, dY, actionState, isCurrentlyActive)
    }

    private fun settleInPlace(viewHolder: RecyclerView.ViewHolder) {
        if (attached != null) helper.onChildViewDetachedFromWindow(viewHolder.itemView)
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
