package dev.proflyder.home.core.designsystem

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.ViewGroup
import androidx.core.view.isGone

class ButtonRow @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : ViewGroup(context, attrs) {
    private val gap = resources.getDimensionPixelSize(R.dimen.ds_space_s)

    var isStacked: Boolean = false
        private set

    private val shown: List<View> get() = (0 until childCount).map(::getChildAt).filterNot { it.isGone }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val available = MeasureSpec.getSize(widthMeasureSpec) - paddingLeft - paddingRight
        val buttons = shown
        buttons.forEach { it.measure(UNBOUNDED, UNBOUNDED) }
        val rowWidth = buttons.sumOf { it.measuredWidth } + gap * (buttons.size - 1).coerceAtLeast(0)
        isStacked = MeasureSpec.getMode(widthMeasureSpec) != MeasureSpec.UNSPECIFIED && rowWidth > available
        val height = when {
            isStacked -> measureStacked(buttons, available)
            else -> buttons.maxOfOrNull { it.measuredHeight } ?: 0
        }
        val width = when {
            isStacked -> available
            else -> rowWidth
        }
        setMeasuredDimension(
            resolveSize(width + paddingLeft + paddingRight, widthMeasureSpec),
            resolveSize(height + paddingTop + paddingBottom, heightMeasureSpec),
        )
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        when {
            isStacked -> layoutStacked(shown.asReversed())
            else -> layoutRow(shown)
        }
    }

    private fun measureStacked(buttons: List<View>, available: Int): Int {
        val exact = MeasureSpec.makeMeasureSpec(available, MeasureSpec.EXACTLY)
        buttons.forEach { it.measure(exact, MeasureSpec.makeMeasureSpec(it.measuredHeight, MeasureSpec.EXACTLY)) }
        return buttons.sumOf { it.measuredHeight } + gap * (buttons.size - 1).coerceAtLeast(0)
    }

    private fun layoutRow(buttons: List<View>) {
        var left = paddingLeft
        buttons.forEach { button ->
            button.layout(left, paddingTop, left + button.measuredWidth, paddingTop + button.measuredHeight)
            left += button.measuredWidth + gap
        }
    }

    private fun layoutStacked(buttons: List<View>) {
        var top = paddingTop
        buttons.forEach { button ->
            button.layout(paddingLeft, top, paddingLeft + button.measuredWidth, top + button.measuredHeight)
            top += button.measuredHeight + gap
        }
    }

    private companion object {
        val UNBOUNDED = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED)
    }
}
