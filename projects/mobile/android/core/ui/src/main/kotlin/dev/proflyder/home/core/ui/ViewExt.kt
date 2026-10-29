package dev.proflyder.home.core.ui

import android.view.View
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import com.google.android.material.color.MaterialColors

/** VISIBLE when [visible], otherwise GONE. */
fun View.setVisible(visible: Boolean) {
    visibility = if (visible) View.VISIBLE else View.GONE
}

/** Wires pull-to-refresh with the app palette (spinner in primary on a raised surface). */
fun SwipeRefreshLayout.setupPullToRefresh(onRefresh: () -> Unit) {
    setColorSchemeColors(MaterialColors.getColor(this, androidx.appcompat.R.attr.colorPrimary))
    setProgressBackgroundColorSchemeColor(
        MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurfaceContainerHigh),
    )
    setOnRefreshListener(onRefresh)
}
