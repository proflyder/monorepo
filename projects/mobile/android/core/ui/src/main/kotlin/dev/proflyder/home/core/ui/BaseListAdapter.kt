package dev.proflyder.home.core.ui

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView

/**
 * Item events of a list screen. An adapter receives ONE instance of its
 * callback (no per-item or per-bind lambdas); holders register listeners once
 * and forward the bound item.
 */
interface AdapterCallback<T> {
    fun onItemClick(item: T)
    fun onItemLongClick(item: T)
}

/** No-op [AdapterCallback]; extend it and override only the events a screen handles. */
open class BaseAdapterCallback<T> : AdapterCallback<T> {
    override fun onItemClick(item: T) = Unit
    override fun onItemLongClick(item: T) = Unit
}

/**
 * ViewBinding-backed holder bound through [bind] (full bind) or [bindPayloads]
 * (partial update produced by the adapter's DiffUtil `getChangePayload`).
 */
abstract class BaseViewHolder<T : Any, VB : ViewBinding, CB : AdapterCallback<T>>(
    protected val binding: VB,
    protected val callback: CB,
) : RecyclerView.ViewHolder(binding.root) {
    private var boundItem: T? = null

    /** The item currently bound; valid inside bind callbacks and listeners. */
    protected val item: T get() = checkNotNull(boundItem) { "ViewHolder used before bind" }

    internal fun performBind(item: T) {
        boundItem = item
        bind(item)
    }

    internal fun performBind(item: T, payloads: List<Any>) {
        boundItem = item
        bindPayloads(item, payloads)
    }

    protected abstract fun bind(item: T)

    /** Partial update; defaults to a full [bind] when a holder does not handle payloads. */
    protected open fun bindPayloads(item: T, payloads: List<Any>) = bind(item)

    /** Opt in to [AdapterCallback.onItemClick]/[AdapterCallback.onItemLongClick] on the whole row, once. */
    protected fun forwardRowClicks() {
        itemView.setOnClickListener { callback.onItemClick(item) }
        itemView.setOnLongClickListener {
            callback.onItemLongClick(item)
            true
        }
    }
}

/**
 * Base for ListAdapter screens. The DiffUtil callback is a required
 * constructor argument (it must define item identity and content equality and
 * may provide payloads so changed rows are updated in place without a rebind).
 *
 * View types: override [viewTypeOf] to classify an item and create the matching
 * holder in [onCreateHolder] (`viewType` is what [viewTypeOf] returned). For
 * several types with different layouts declare `VB` as the common
 * [ViewBinding] and let each holder cast its own binding.
 */
abstract class BaseListAdapter<T : Any, VB : ViewBinding, CB : AdapterCallback<T>>(
    protected val callback: CB,
    diff: DiffUtil.ItemCallback<T>,
) : ListAdapter<T, BaseViewHolder<T, VB, CB>>(diff) {

    /** View type of [item]; a single type (0) by default. */
    protected open fun viewTypeOf(item: T): Int = 0

    final override fun getItemViewType(position: Int): Int = viewTypeOf(getItem(position))

    final override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): BaseViewHolder<T, VB, CB> =
        onCreateHolder(LayoutInflater.from(parent.context), parent, viewType)

    protected abstract fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<T, VB, CB>

    final override fun onBindViewHolder(holder: BaseViewHolder<T, VB, CB>, position: Int) {
        holder.performBind(getItem(position))
    }

    final override fun onBindViewHolder(
        holder: BaseViewHolder<T, VB, CB>,
        position: Int,
        payloads: MutableList<Any>,
    ) {
        if (payloads.isEmpty()) holder.performBind(getItem(position)) else holder.performBind(getItem(position), payloads)
    }
}
