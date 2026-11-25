package dev.proflyder.home.feature.chat.presentation

import android.graphics.Canvas
import android.graphics.drawable.Drawable
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.HomeMotion
import dev.proflyder.home.feature.chat.impl.R
import kotlin.math.abs

internal class SwipeReplyMath(private val threshold: Float, private val maxOffset: Float) {
    fun offset(dragX: Float): Float = dragX.coerceIn(-maxOffset, 0f)

    fun progress(offset: Float): Float = (abs(offset) / threshold).coerceIn(0f, 1f)

    fun crossed(offset: Float): Boolean = abs(offset) >= threshold

    fun startsSwipe(dx: Float, dy: Float, slop: Int): Boolean = dx < -slop && abs(dx) > abs(dy)
}

internal class SwipeReply(
    private val list: RecyclerView,
    private val quotable: (Int) -> Boolean,
    private val onReply: (Int) -> Unit,
) : RecyclerView.ItemDecoration(), RecyclerView.OnItemTouchListener {
    private val resources = list.resources
    private val math = SwipeReplyMath(
        resources.getDimension(R.dimen.chat_swipe_reply_distance),
        resources.getDimension(R.dimen.chat_swipe_reply_max),
    )
    private val slop = ViewConfiguration.get(list.context).scaledTouchSlop
    private val iconSize = resources.getDimensionPixelSize(R.dimen.chat_swipe_reply_icon)
    private val iconMargin = resources.getDimension(R.dimen.chat_swipe_reply_icon_margin)
    private val icon: Drawable = checkNotNull(ContextCompat.getDrawable(list.context, R.drawable.chat_ic_reply)).mutate().apply {
        setTint(MaterialColors.getColor(list, com.google.android.material.R.attr.colorOnSurfaceVariant))
    }
    private var row: View? = null
    private var downX = 0f
    private var downY = 0f
    private var swiping = false
    private var armed = false

    fun attach() {
        list.addOnItemTouchListener(this)
        list.addItemDecoration(this)
    }

    override fun onInterceptTouchEvent(rv: RecyclerView, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> begin(e)
            MotionEvent.ACTION_MOVE -> swiping = swiping || shouldSwipe(e)
        }
        if (swiping) rv.parent?.requestDisallowInterceptTouchEvent(true)
        return swiping
    }

    override fun onTouchEvent(rv: RecyclerView, e: MotionEvent) {
        when (e.actionMasked) {
            MotionEvent.ACTION_MOVE -> drag(e.x - downX)
            MotionEvent.ACTION_UP -> release(commit = true)
            MotionEvent.ACTION_CANCEL -> release(commit = false)
        }
    }

    override fun onRequestDisallowInterceptTouchEvent(disallowIntercept: Boolean) = Unit

    override fun onDrawOver(canvas: Canvas, parent: RecyclerView, state: RecyclerView.State) {
        val view = row ?: return
        val progress = math.progress(view.translationX)
        if (progress <= 0f) return
        val size = (iconSize * progress).toInt()
        val centerX = (view.right - iconMargin - iconSize / 2f).toInt()
        val centerY = (view.top + view.translationY + view.height / 2f).toInt()
        icon.setBounds(centerX - size / 2, centerY - size / 2, centerX + size / 2, centerY + size / 2)
        icon.alpha = (progress * OPAQUE).toInt()
        icon.draw(canvas)
    }

    private fun begin(e: MotionEvent) {
        downX = e.x
        downY = e.y
        swiping = false
        armed = false
        row?.let(::settleNow)
        row = list.findChildViewUnder(e.x, e.y)?.takeIf { quotable(list.getChildAdapterPosition(it)) }
        row?.let(::settleNow)
    }

    private fun settleNow(view: View) {
        view.animate().cancel()
        view.translationX = 0f
    }

    private fun forget(view: View) {
        if (row === view) row = null
        list.invalidate()
    }

    private fun shouldSwipe(e: MotionEvent): Boolean = row != null && math.startsSwipe(e.x - downX, e.y - downY, slop)

    private fun drag(dragX: Float) {
        val view = row ?: return
        view.translationX = math.offset(dragX + slop)
        val crossed = math.crossed(view.translationX)
        if (crossed && !armed) view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
        armed = crossed
        list.invalidate()
    }

    private fun release(commit: Boolean) {
        val view = row ?: return
        val position = list.getChildAdapterPosition(view)
        if (commit && armed && position != RecyclerView.NO_POSITION) onReply(position)
        swiping = false
        armed = false
        view.animate()
            .translationX(0f)
            .setDuration(HomeMotion.exitDuration(view.context).toLong())
            .setInterpolator(HomeMotion.enterInterpolator(view.context))
            .setUpdateListener { list.invalidate() }
            .withEndAction { forget(view) }
            .start()
    }

    private companion object {
        const val OPAQUE = 255
    }
}
