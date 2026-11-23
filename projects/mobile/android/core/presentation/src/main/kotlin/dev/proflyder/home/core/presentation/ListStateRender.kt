package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.LoadState
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.ui.ListStateViews

fun ListStateViews.render(
    load: ListLoadState,
    hasItems: Boolean,
    failureText: (RemoteFailure) -> CharSequence,
) {
    val failure = load.failure
    when {
        hasItems -> showContent()
        load.refreshing -> showLoading()
        failure != null -> showError(failureText(failure))
        else -> showEmpty()
    }
}

fun ListStateViews.render(
    pages: PagedState<*>,
    failureText: (RemoteFailure) -> CharSequence,
) {
    val refresh = pages.refresh
    when {
        pages.items.isNotEmpty() -> showContent(pages.isRefreshing)
        refresh is LoadState.Loading -> showLoading()
        refresh is LoadState.Failed -> showError(failureText(refresh.failure))
        else -> showEmpty()
    }
}
