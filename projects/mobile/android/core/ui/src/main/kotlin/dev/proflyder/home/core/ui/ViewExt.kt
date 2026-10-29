package dev.proflyder.home.core.ui

import android.view.View

/** VISIBLE when [visible], otherwise GONE. */
fun View.setVisible(visible: Boolean) {
    visibility = if (visible) View.VISIBLE else View.GONE
}
