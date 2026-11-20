package dev.proflyder.home.core.designsystem

import android.content.Context
import android.text.StaticLayout
import android.text.TextPaint
import android.util.AttributeSet
import android.util.TypedValue
import androidx.appcompat.widget.AppCompatTextView

class ToolbarTitle @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : AppCompatTextView(context, attrs, R.attr.homeToolbarTitleStyle) {
    private val maxSize = resources.getDimension(R.dimen.ds_toolbar_title_max)
    private val minSize = resources.getDimension(R.dimen.ds_toolbar_title_min)
    private val step = resources.getDimension(R.dimen.ds_toolbar_title_step)
    private val probe = TextPaint()

    init {
        maxLines = MAX_LINES
        setTextSize(TypedValue.COMPLEX_UNIT_PX, maxSize)
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec) - totalPaddingLeft - totalPaddingRight
        val height = MeasureSpec.getSize(heightMeasureSpec) - totalPaddingTop - totalPaddingBottom
        val bounded = MeasureSpec.getMode(widthMeasureSpec) != MeasureSpec.UNSPECIFIED && width > 0
        val size = when {
            bounded -> fittingSize(width, height, MeasureSpec.getMode(heightMeasureSpec) != MeasureSpec.UNSPECIFIED)
            else -> maxSize
        }
        if (size != textSize) setTextSize(TypedValue.COMPLEX_UNIT_PX, size)
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
    }

    private fun fittingSize(width: Int, height: Int, limitHeight: Boolean): Float =
        generateSequence(maxSize) { it - step }
            .takeWhile { it >= minSize }
            .firstOrNull { fits(it, width, height.takeIf { limitHeight }) }
            ?: minSize

    internal fun fits(size: Float, width: Int, height: Int?): Boolean {
        probe.set(paint)
        probe.textSize = size
        val text = text ?: return true
        val wordsFit = text.split(' ').all { probe.measureText(it) <= width }
        val layout = layoutFor(text, width)
        return wordsFit && layout.lineCount <= MAX_LINES && (height == null || layout.height <= height)
    }

    private fun layoutFor(text: CharSequence, width: Int): StaticLayout =
        StaticLayout.Builder.obtain(text, 0, text.length, probe, width)
            .setBreakStrategy(breakStrategy)
            .setHyphenationFrequency(hyphenationFrequency)
            .setIncludePad(includeFontPadding)
            .setLineSpacing(lineSpacingExtra, lineSpacingMultiplier)
            .build()

    private companion object {
        const val MAX_LINES = 2
    }
}
