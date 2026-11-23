package dev.proflyder.home.core.presentation

import androidx.recyclerview.widget.ConcatAdapter
import androidx.recyclerview.widget.RecyclerView
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.PagingFooterAdapter
import dev.proflyder.home.core.ui.footer
import dev.proflyder.home.core.ui.loadMoreNearEnd

class PagingBinder<T : Keyed>(
    private val recyclerView: RecyclerView,
    private val states: ListStateViews,
    content: RecyclerView.Adapter<*>,
    private val failureText: (RemoteFailure) -> CharSequence,
    private val dispatch: (PagingIntent) -> Unit,
    private val refreshLayout: SwipeRefreshLayout? = null,
) {
    private val footer = PagingFooterAdapter { dispatch(PagingIntent.Retry) }

    init {
        recyclerView.adapter = ConcatAdapter(content, footer)
        recyclerView.loadMoreNearEnd { dispatch(PagingIntent.LoadMore) }
        refreshLayout?.setOnRefreshListener { dispatch(PagingIntent.Refresh(RefreshMode.HARD)) }
    }

    fun render(pages: PagedState<T>) {
        states.render(pages, failureText)
        footer.render(pages.footer())
        refreshLayout?.isRefreshing = pages.isRefreshing
    }
}
