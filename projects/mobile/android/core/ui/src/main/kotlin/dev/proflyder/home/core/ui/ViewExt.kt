package dev.proflyder.home.core.ui

import android.view.View
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import com.google.android.material.color.MaterialColors

fun View.setVisible(visible: Boolean) {
    visibility = if (visible) View.VISIBLE else View.GONE
}

fun SwipeRefreshLayout.setupPullToRefresh(scrolling: View, vararg alsoScrolling: View, onRefresh: () -> Unit) {
    setColorSchemeColors(MaterialColors.getColor(this, androidx.appcompat.R.attr.colorPrimary))
    setProgressBackgroundColorSchemeColor(
        MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurfaceContainerHigh),
    )
    val tracked = listOf(scrolling) + alsoScrolling
    setOnChildScrollUpCallback { _, _ -> tracked.any(View::isAwayFromTop) }
    setOnRefreshListener(onRefresh)
}

internal fun View.isAwayFromTop(): Boolean = canScrollVertically(-1) || top < 0
