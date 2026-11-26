package dev.proflyder.home.core.designsystem.search

import android.animation.ValueAnimator
import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.FrameLayout
import androidx.core.widget.doAfterTextChanged
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.HomeMotion
import kotlin.math.roundToInt

class PullRevealSearch @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {
    private var animator: ValueAnimator? = null
    private var input: EditText? = null
    private var expanded = false
    private var dragging = false

    var offset: Int = 0
        private set

    var fullHeight: Int = 0
        private set

    val fraction: Float get() = if (fullHeight == 0) 0f else offset.toFloat() / fullHeight

    val isRevealed: Boolean get() = expanded && !dragging

    var onRevealChanged: ((Boolean) -> Unit)? = null

    fun attach(list: RecyclerView, field: EditText) {
        input = field
        val gesture = PullRevealGesture(this, list)
        list.addOnItemTouchListener(gesture)
        list.addOnScrollListener(gesture)
        field.setOnFocusChangeListener { _, _ -> collapseIfIdle() }
        field.doAfterTextChanged { if (!it.isNullOrEmpty()) expand() else collapseIfIdle() }
        if (!field.text.isNullOrEmpty()) expand(animate = false)
    }

    fun expand(animate: Boolean = true) = settle(true, animate)

    fun collapse(animate: Boolean = true) = settle(false, animate)

    val idle: Boolean get() = input?.let { it.text.isNullOrEmpty() && !it.hasFocus() } ?: true

    fun collapseIfIdle() {
        if (idle) collapse()
    }

    val blank: Boolean get() = input?.text.isNullOrEmpty()

    fun releaseFocusIfBlank() {
        if (blank) releaseFocus()
    }

    fun dismiss() {
        input?.text?.clear()
        releaseFocus()
        collapse()
    }

    private fun releaseFocus() {
        val field = input ?: return
        if (!field.hasFocus()) return
        field.clearFocus()
        context.getSystemService(InputMethodManager::class.java)?.hideSoftInputFromWindow(field.windowToken, 0)
    }

    override fun canScrollVertically(direction: Int): Boolean = when {
        direction < 0 -> !isRevealed
        else -> super.canScrollVertically(direction)
    }

    internal fun beginDrag() {
        animator?.cancel()
        dragging = true
    }

    internal fun dragTo(value: Int) = applyOffset(value.coerceIn(0, fullHeight))

    internal fun endDrag() {
        dragging = false
        settle(fraction >= SETTLE_THRESHOLD, animate = true)
    }

    private fun settle(open: Boolean, animate: Boolean) {
        if (expanded != open) onRevealChanged?.invoke(open)
        expanded = open
        animator?.cancel()
        val target = if (open) fullHeight else 0
        if (!animate || !isLaidOut || !ValueAnimator.areAnimatorsEnabled()) return applyOffset(target)
        if (target == offset) return
        animator = ValueAnimator.ofInt(offset, target).apply {
            duration = (if (open) HomeMotion.enterDuration(context) else HomeMotion.exitDuration(context)).toLong()
            interpolator = if (open) HomeMotion.enterInterpolator(context) else HomeMotion.exitInterpolator(context)
            addUpdateListener { applyOffset(it.animatedValue as Int) }
            start()
        }
    }

    private fun applyOffset(value: Int) {
        if (value == offset) return
        offset = value
        requestLayout()
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val content = getChildAt(0) ?: return super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        measureChildWithMargins(content, widthMeasureSpec, 0, MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED), 0)
        val params = content.layoutParams as LayoutParams
        fullHeight = content.measuredHeight + params.topMargin + params.bottomMargin + paddingTop + paddingBottom
        if (expanded && !dragging && animator?.isRunning != true) offset = fullHeight
        offset = offset.coerceIn(0, fullHeight)
        setMeasuredDimension(MeasureSpec.getSize(widthMeasureSpec), offset)
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val content = getChildAt(0) ?: return
        val params = content.layoutParams as LayoutParams
        val childTop = offset - fullHeight + paddingTop + params.topMargin
        val childLeft = paddingLeft + params.leftMargin
        content.layout(childLeft, childTop, childLeft + content.measuredWidth, childTop + content.measuredHeight)
        content.alpha = fraction
        content.visibility = if (offset == 0) View.INVISIBLE else View.VISIBLE
    }

    internal fun dragOffsetFor(distance: Float): Int = (distance * DRAG_RATIO).roundToInt()

    private companion object {
        const val SETTLE_THRESHOLD = 0.5f
        const val DRAG_RATIO = 0.6f
    }
}
