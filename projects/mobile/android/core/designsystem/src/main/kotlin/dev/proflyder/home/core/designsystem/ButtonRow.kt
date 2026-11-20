package dev.proflyder.home.core.designsystem

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.ViewGroup
import androidx.core.content.withStyledAttributes
import androidx.core.view.isGone

class ButtonRow @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : ViewGroup(context, attrs) {
    private val gap = resources.getDimensionPixelSize(R.dimen.ds_space_s)

    var equalWidth: Boolean = false
    var centered: Boolean = false

    var isStacked: Boolean = false
        private set

    private var contentWidth = 0

    init {
        context.withStyledAttributes(attrs, R.styleable.ButtonRow) {
            equalWidth = getBoolean(R.styleable.ButtonRow_equalWidth, false)
            centered = getBoolean(R.styleable.ButtonRow_centered, false)
        }
    }

    private val shown: List<View> get() = (0 until childCount).map(::getChildAt).filterNot { it.isGone }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val available = MeasureSpec.getSize(widthMeasureSpec) - paddingLeft - paddingRight
        val buttons = shown
        buttons.forEach { it.measure(UNBOUNDED, UNBOUNDED) }
        val widest = buttons.maxOfOrNull { it.measuredWidth } ?: 0
        val gaps = gap * (buttons.size - 1).coerceAtLeast(0)
        val rowWidth = when {
            equalWidth -> widest * buttons.size + gaps
            else -> buttons.sumOf { it.measuredWidth } + gaps
        }
        isStacked = MeasureSpec.getMode(widthMeasureSpec) != MeasureSpec.UNSPECIFIED && rowWidth > available
        contentWidth = when {
            isStacked && equalWidth -> widest.coerceAtMost(available)
            isStacked -> available
            else -> rowWidth
        }
        val height = when {
            isStacked -> measureAll(buttons, contentWidth) * buttons.size + gaps
            equalWidth -> measureAll(buttons, widest)
            else -> buttons.maxOfOrNull { it.measuredHeight } ?: 0
        }
        val width = when {
            centered -> available
            else -> contentWidth
        }
        setMeasuredDimension(
            resolveSize(width + paddingLeft + paddingRight, widthMeasureSpec),
            resolveSize(height + paddingTop + paddingBottom, heightMeasureSpec),
        )
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        val start = paddingLeft + offset(r - l - paddingLeft - paddingRight)
        when {
            isStacked -> layoutStacked(shown.asReversed(), start)
            else -> layoutRow(shown, start)
        }
    }

    private fun offset(width: Int): Int = when {
        centered -> ((width - contentWidth) / 2).coerceAtLeast(0)
        else -> 0
    }

    private fun measureAll(buttons: List<View>, width: Int): Int {
        val exact = MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY)
        buttons.forEach { it.measure(exact, MeasureSpec.makeMeasureSpec(it.measuredHeight, MeasureSpec.EXACTLY)) }
        return buttons.maxOfOrNull { it.measuredHeight } ?: 0
    }

    private fun layoutRow(buttons: List<View>, start: Int) {
        var left = start
        buttons.forEach { button ->
            button.layout(left, paddingTop, left + button.measuredWidth, paddingTop + button.measuredHeight)
            left += button.measuredWidth + gap
        }
    }

    private fun layoutStacked(buttons: List<View>, start: Int) {
        var top = paddingTop
        buttons.forEach { button ->
            button.layout(start, top, start + button.measuredWidth, top + button.measuredHeight)
            top += button.measuredHeight + gap
        }
    }

    private companion object {
        val UNBOUNDED = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED)
    }
}
