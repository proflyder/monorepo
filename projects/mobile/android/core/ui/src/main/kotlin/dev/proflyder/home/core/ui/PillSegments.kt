package dev.proflyder.home.core.ui

import android.content.Context
import android.util.AttributeSet
import android.view.LayoutInflater
import android.widget.LinearLayout
import android.widget.TextView

/**
 * A row of compact text pills: one is selected and only it has a background
 * (the same stadium as the bottom navigation indicator), the others are plain
 * labels. Used for the Time sections and the voice profile, so both switches
 * look identical. It is a plain row of TextViews on purpose: TabLayout brings a
 * minimum height and a bottom divider that do not belong to this design.
 */
class PillSegments @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {

    /** Called when the user taps another pill; not called for [select]. */
    var onSelected: ((Int) -> Unit)? = null

    var selectedIndex: Int = 0
        private set

    init {
        orientation = HORIZONTAL
    }

    fun setLabels(labels: List<CharSequence>) {
        removeAllViews()
        labels.forEachIndexed { index, label -> addView(pill(index, label)) }
        select(selectedIndex.coerceIn(0, (labels.size - 1).coerceAtLeast(0)))
    }

    /** Programmatic selection (state from the model or a pager swipe). */
    fun select(index: Int) {
        selectedIndex = index
        for (child in 0 until childCount) {
            getChildAt(child).isSelected = child == index
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

    private fun pill(index: Int, label: CharSequence): TextView {
        val view = LayoutInflater.from(context).inflate(R.layout.view_pill_segment, this, false) as TextView
        view.text = label
        view.setOnClickListener { tap(index) }
        return view
    }
}
