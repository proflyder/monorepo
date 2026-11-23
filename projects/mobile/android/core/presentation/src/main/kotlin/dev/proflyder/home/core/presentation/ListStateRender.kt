package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.PagingState
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
    paging: PagingState<*>,
    failureText: (RemoteFailure) -> CharSequence,
) {
    val failure = paging.failure
    when {
        paging.hasItems -> showContent(paging.refreshing)
        paging.refreshing -> showLoading()
        failure != null -> showError(failureText(failure))
        else -> showEmpty()
    }
}
