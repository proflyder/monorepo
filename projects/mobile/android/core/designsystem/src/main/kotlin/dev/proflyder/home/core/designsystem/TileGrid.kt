package dev.proflyder.home.core.designsystem

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.ViewGroup
import androidx.core.content.withStyledAttributes
import androidx.core.view.isGone

class TileGrid @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : ViewGroup(context, attrs) {
    var columns: Int = DEFAULT_COLUMNS
        set(value) {
            field = value.coerceAtLeast(1)
            requestLayout()
        }

    init {
        context.withStyledAttributes(attrs, R.styleable.TileGrid) {
            columns = getInt(R.styleable.TileGrid_tileColumns, DEFAULT_COLUMNS)
        }
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val cell = cellWidth(width)
        val visible = visibleChildren()
        visible.forEach { measureCell(it, cell, heightMeasureSpec) }
        val height = visible.chunked(columns).sumOf { row -> row.maxOf(View::getMeasuredHeight) }
        setMeasuredDimension(width, resolveSize(height + paddingTop + paddingBottom, heightMeasureSpec))
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val cell = cellWidth(right - left)
        var y = paddingTop
        visibleChildren().chunked(columns).forEach { row -> y += layoutRow(row, y, cell) }
    }

    private fun layoutRow(row: List<View>, y: Int, cell: Int): Int {
        row.forEachIndexed { column, child -> placeCell(child, column, y, cell) }
        return row.maxOf(View::getMeasuredHeight)
    }

    private fun placeCell(child: View, column: Int, y: Int, cell: Int) {
        val x = when (layoutDirection) {
            LAYOUT_DIRECTION_RTL -> width - paddingRight - cell * (column + 1)
            else -> paddingLeft + cell * column
        }
        child.layout(x, y, x + child.measuredWidth, y + child.measuredHeight)
    }

    private fun measureCell(child: View, cell: Int, heightMeasureSpec: Int) {
        val childHeight = getChildMeasureSpec(heightMeasureSpec, 0, child.layoutParams.height)
        child.measure(MeasureSpec.makeMeasureSpec(cell, MeasureSpec.EXACTLY), childHeight)
    }

    private fun cellWidth(width: Int): Int = (width - paddingLeft - paddingRight).coerceAtLeast(0) / columns

    private fun visibleChildren(): List<View> = (0 until childCount).map(::getChildAt).filterNot(View::isGone)

    override fun checkLayoutParams(params: LayoutParams?): Boolean = params != null

    override fun generateDefaultLayoutParams(): LayoutParams =
        LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT)

    override fun generateLayoutParams(attrs: AttributeSet?): LayoutParams = LayoutParams(context, attrs)

    override fun generateLayoutParams(params: LayoutParams?): LayoutParams =
        LayoutParams(params ?: generateDefaultLayoutParams())

    private companion object {
        const val DEFAULT_COLUMNS = 4
    }
}
