package dev.proflyder.home.core.presentation

import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
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
import kotlinx.coroutines.launch

class PagingBinder<T : Keyed>(
    private val recyclerView: RecyclerView,
    private val states: ListStateViews,
    content: RecyclerView.Adapter<*>,
    private val lifecycleOwner: LifecycleOwner,
    private val handler: PagingHandler<T>,
    private val failureText: (RemoteFailure) -> CharSequence,
    private val dispatch: (PagingIntent) -> Unit,
    private val refreshLayout: SwipeRefreshLayout? = null,
) {
    private val footer = PagingFooterAdapter { dispatch(PagingIntent.Retry) }

    init {
        recyclerView.adapter = ConcatAdapter(content, footer)
        recyclerView.loadMoreNearEnd { dispatch(PagingIntent.LoadMore) }
        refreshLayout?.setOnRefreshListener { dispatch(PagingIntent.Refresh(RefreshMode.HARD)) }
        lifecycleOwner.lifecycleScope.launch {
            lifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
                handler.state.collect(::render)
            }
        }
    }

    private fun render(pages: PagedState<T>) {
        states.render(pages, failureText)
        footer.render(pages.footer())
        refreshLayout?.isRefreshing = pages.isRefreshing
    }
}
