package dev.proflyder.home.core.ui

import android.view.View
import android.widget.TextView

/**
 * Shared loading/empty/error/content rendering for list screens. Exactly one of
 * the four regions is visible; the in-flight refresh over existing content is
 * expressed with [showContent]`(refreshing = true)` rather than a separate state.
 *
 * [error] is the container (message + actions); [errorText] receives the text.
 */
class ListStateViews(
    private val progress: View,
    private val empty: View,
    private val error: View,
    private val content: View,
    private val errorText: TextView? = error as? TextView,
) {
    fun showLoading() = show(loading = true)

    fun showEmpty(emptyText: CharSequence? = null) {
        if (emptyText != null && empty is TextView) empty.text = emptyText
        show(empty = true)
    }

    fun showContent(refreshing: Boolean = false) = show(content = true, loading = refreshing)

    fun showError(message: CharSequence) {
        errorText?.text = message
        show(error = true)
    }

    private fun show(
        loading: Boolean = false,
        empty: Boolean = false,
        error: Boolean = false,
        content: Boolean = false,
    ) {
        progress.visibility = visible(loading)
        this.empty.visibility = visible(empty)
        this.error.visibility = visible(error)
        this.content.visibility = visible(content)
    }

    private fun visible(value: Boolean) = if (value) View.VISIBLE else View.GONE
}
