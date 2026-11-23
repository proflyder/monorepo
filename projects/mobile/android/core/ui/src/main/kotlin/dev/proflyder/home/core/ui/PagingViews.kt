package dev.proflyder.home.core.ui

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.data.LoadState
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.ui.databinding.ItemPagingFooterBinding

enum class PagingFooter {
    NONE,
    LOADING,
    ERROR,
}

fun PagedState<*>.footer(): PagingFooter = when (append) {
    is LoadState.Failed -> PagingFooter.ERROR
    LoadState.Loading -> PagingFooter.LOADING
    else -> PagingFooter.NONE
}

class PagingFooterAdapter(private val onRetry: () -> Unit) :
    RecyclerView.Adapter<PagingFooterAdapter.FooterHolder>() {
    private var footer: PagingFooter = PagingFooter.NONE

    fun render(next: PagingFooter) {
        if (next == footer) return
        val had = current()
        footer = next
        val has = current()
        when {
            had && has -> notifyItemChanged(0)
            has -> notifyItemInserted(0)
            had -> notifyItemRemoved(0)
        }
    }

    override fun getItemCount(): Int = if (current()) 1 else 0

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): FooterHolder =
        FooterHolder(ItemPagingFooterBinding.inflate(LayoutInflater.from(parent.context), parent, false)) { onRetry() }

    override fun onBindViewHolder(holder: FooterHolder, position: Int) = holder.bind(footer)

    class FooterHolder(
        private val views: ItemPagingFooterBinding,
        private val retry: () -> Unit,
    ) : RecyclerView.ViewHolder(views.root) {
        init {
            views.pagingFooterRetry.setOnClickListener { retry() }
        }

        fun bind(footer: PagingFooter) {
            views.pagingFooterProgress.visibility = visible(footer == PagingFooter.LOADING)
            views.pagingFooterError.visibility = visible(footer == PagingFooter.ERROR)
            views.pagingFooterRetry.visibility = visible(footer == PagingFooter.ERROR)
            if (footer == PagingFooter.ERROR) {
                views.pagingFooterError.setText(R.string.ui_paging_error)
            }
        }

        private fun visible(value: Boolean): Int = if (value) View.VISIBLE else View.GONE
    }

    private fun current(): Boolean = footer != PagingFooter.NONE
}

fun RecyclerView.loadMoreNearEnd(threshold: Int = DEFAULT_LOAD_MORE_THRESHOLD, action: () -> Unit) {
    addOnScrollListener(object : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
            if (dy <= 0) return
            val manager = recyclerView.layoutManager as? LinearLayoutManager ?: return
            val last = manager.findLastVisibleItemPosition()
            if (last >= recyclerView.adapter.orEmptyItemCount() - 1 - threshold) action()
        }
    })
}

private fun RecyclerView.Adapter<*>?.orEmptyItemCount(): Int = this?.itemCount ?: 0

const val DEFAULT_LOAD_MORE_THRESHOLD: Int = 3
