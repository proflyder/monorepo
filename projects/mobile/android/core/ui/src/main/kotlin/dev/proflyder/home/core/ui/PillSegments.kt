package dev.proflyder.home.core.ui

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.AttributeSet
import android.view.LayoutInflater
import android.widget.LinearLayout
import android.widget.TextView
import com.google.android.material.color.MaterialColors

/**
 * A row of compact text pills: the selected one sits on a stadium (the same
 * shape and colour as the bottom navigation indicator) that slides to the
 * newly selected pill. Used for the Time sections and the voice profile, so
 * both switches look identical. It is a plain row of TextViews on purpose:
 * TabLayout brings a minimum height and a bottom divider that do not belong
 * to this design.
 */
class PillSegments @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {

    /** Called when the user taps another pill; not called for [select]. */
    var onSelected: ((Int) -> Unit)? = null

    var selectedIndex: Int = 0
        private set

    private val indicator = RectF()
    private val indicatorPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = MaterialColors.getColor(this@PillSegments, com.google.android.material.R.attr.colorSecondaryContainer)
    }
    private var animator: ValueAnimator? = null

    init {
        orientation = HORIZONTAL
        setWillNotDraw(false)
    }

    fun setLabels(labels: List<CharSequence>) {
        removeAllViews()
        labels.forEachIndexed { index, label -> addView(pill(index, label)) }
        select(selectedIndex.coerceIn(NONE, labels.size - 1))
    }

    /** Programmatic selection (state from the model or a pager swipe); [NONE] clears it. */
    fun select(index: Int) {
        val previous = selectedIndex
        selectedIndex = index
        for (child in 0 until childCount) {
            getChildAt(child).isSelected = child == index
        }
        when (isLaidOut && previous != index && previous in 0 until childCount && index in 0 until childCount) {
            true -> slide(previous, index)
            false -> snap()
        }
    }

    internal fun tap(index: Int) {
        when (index == selectedIndex || !isEnabled) {
            true -> Unit
            false -> {
                select(index)
                onSelected?.invoke(index)
            }
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        super.onLayout(changed, l, t, r, b)
        when (animator?.isRunning == true) {
            true -> Unit
            false -> snap()
        }
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        when (indicator.isEmpty) {
            true -> Unit
            false -> canvas.drawRoundRect(indicator, indicator.height() / 2, indicator.height() / 2, indicatorPaint)
        }
    }

    override fun onDetachedFromWindow() {
        animator?.cancel()
        super.onDetachedFromWindow()
    }

    private fun snap() {
        animator?.cancel()
        bounds(selectedIndex, indicator)
        invalidate()
    }

    private fun slide(from: Int, to: Int) {
        val start = RectF().also { bounds(from, it) }
        val end = RectF().also { bounds(to, it) }
        animator?.cancel()
        animator = ValueAnimator.ofFloat(0f, 1f).apply {
            duration = HomeMotion.selectionDuration(context).toLong()
            interpolator = HomeMotion.selectionInterpolator(context)
            addUpdateListener { lerp(start, end, it.animatedValue as Float) }
            start()
        }
    }

    internal fun lerp(start: RectF, end: RectF, fraction: Float) {
        indicator.set(
            start.left + (end.left - start.left) * fraction,
            start.top + (end.top - start.top) * fraction,
            start.right + (end.right - start.right) * fraction,
            start.bottom + (end.bottom - start.bottom) * fraction,
        )
        invalidate()
    }

    private fun bounds(index: Int, out: RectF) {
        val child = getChildAt(index)
        when (child) {
            null -> out.setEmpty()
            else -> out.set(child.left.toFloat(), child.top.toFloat(), child.right.toFloat(), child.bottom.toFloat())
        }
    }

    private fun pill(index: Int, label: CharSequence): TextView {
        val view = LayoutInflater.from(context).inflate(R.layout.view_pill_segment, this, false) as TextView
        view.text = label
        view.setOnClickListener { tap(index) }
        return view
    }

    companion object {
        /** No pill selected (state unknown). */
        const val NONE = -1
    }
}
