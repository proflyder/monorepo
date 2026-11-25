package dev.proflyder.home.core.ui

import android.view.View
import androidx.core.view.ViewCompat
import androidx.recyclerview.widget.RecyclerView
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
    ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
        (view as SwipeRefreshLayout).placeSpinnerBelow(insets.topInset())
        insets
    }
    requestInsetsWhenAttached()
}

private fun SwipeRefreshLayout.placeSpinnerBelow(top: Int) {
    val travel = resources.getDimensionPixelSize(R.dimen.ui_refresh_travel)
    setProgressViewOffset(false, top - progressCircleDiameter, top + travel)
}

fun RecyclerView.isAtTop(): Boolean {
    val first = getChildAt(0) ?: return true
    val manager = layoutManager ?: return !canScrollVertically(-1)
    return getChildAdapterPosition(first) == 0 && manager.getDecoratedTop(first) >= paddingTop
}

internal fun View.isAwayFromTop(): Boolean = when (this) {
    is RecyclerView -> !isAtTop()
    else -> canScrollVertically(-1) || top < 0
}
