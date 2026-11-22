package dev.proflyder.home.core.designsystem.list

import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.view.View
import androidx.annotation.Px
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import com.google.android.material.shape.MaterialShapeDrawable
import com.google.android.material.shape.ShapeAppearanceModel

class GroupDividers(@Px private val insetStart: Int = 0) : RecyclerView.ItemDecoration() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)

    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        paint.color = MaterialColors.getColor(parent, com.google.android.material.R.attr.colorOutlineVariant)
        val thickness = parent.resources.displayMetrics.density.coerceAtLeast(1f)
        parent.children().filter { parent.slotOfChild(it)?.last == false }.forEach { child ->
            val bottom = child.bottom + child.translationY
            canvas.drawRect(child.left + insetStart.toFloat(), bottom - thickness, child.right.toFloat(), bottom, paint)
        }
    }
}

class GroupCards(
    @Px private val radius: Float,
    @Px private val groupGap: Int,
) : RecyclerView.ItemDecoration() {
    override fun getItemOffsets(outRect: Rect, view: View, parent: RecyclerView, state: RecyclerView.State) {
        if (parent.slotOfChild(view)?.last == true) outRect.bottom = groupGap
    }

    private val backgrounds = mutableMapOf<GroupSlot, MaterialShapeDrawable>()

    override fun onDraw(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        val color = MaterialColors.getColor(parent, com.google.android.material.R.attr.colorSurfaceContainer)
        parent.children().forEach { child -> parent.slotOfChild(child)?.let { drawBackground(canvas, child, it, color) } }
    }

    private fun drawBackground(canvas: Canvas, child: View, slot: GroupSlot, color: Int) {
        val background = backgrounds.getOrPut(slot) { shapeFor(slot) }
        background.fillColor = ColorStateList.valueOf(color)
        val shift = child.translationY.toInt()
        background.setBounds(child.left, child.top + shift, child.right, child.bottom + shift)
        background.alpha = (child.alpha * OPAQUE).toInt()
        background.draw(canvas)
    }

    private fun shapeFor(slot: GroupSlot): MaterialShapeDrawable {
        val top = if (slot.first) radius else 0f
        val bottom = if (slot.last) radius else 0f
        val shape = ShapeAppearanceModel.builder()
            .setTopLeftCornerSize(top)
            .setTopRightCornerSize(top)
            .setBottomLeftCornerSize(bottom)
            .setBottomRightCornerSize(bottom)
            .build()
        return MaterialShapeDrawable(shape)
    }

    private companion object {
        const val OPAQUE = 255
    }
}

internal fun RecyclerView.children(): List<View> = (0 until childCount).map(::getChildAt)
