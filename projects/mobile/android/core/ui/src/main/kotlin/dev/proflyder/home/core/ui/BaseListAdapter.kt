package dev.proflyder.home.core.ui

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView

interface AdapterCallback<T> {
    fun onItemClick(item: T)
    fun onItemLongClick(item: T)
}

open class BaseAdapterCallback<T> : AdapterCallback<T> {
    override fun onItemClick(item: T) = Unit
    override fun onItemLongClick(item: T) = Unit
}

abstract class BaseViewHolder<T : Any, VB : ViewBinding, CB : AdapterCallback<T>>(
    protected val binding: VB,
    protected val callback: CB,
) : RecyclerView.ViewHolder(binding.root) {
    private var boundItem: T? = null

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

    protected open fun bindPayloads(item: T, payloads: List<Any>) = bind(item)

    protected fun forwardRowClicks() {
        itemView.setOnClickListener { callback.onItemClick(item) }
        itemView.setOnLongClickListener {
            callback.onItemLongClick(item)
            true
        }
    }
}

class PayloadSet<A : Enum<A>> private constructor(private val changed: Set<A>) {
    val aspects: Set<A> get() = changed

    operator fun contains(aspect: A): Boolean = aspect in changed

    override fun equals(other: Any?): Boolean = other is PayloadSet<*> && other.changed == changed

    override fun hashCode(): Int = changed.hashCode()

    override fun toString(): String = "PayloadSet(${changed.joinToString(",") { it.name }})"

    companion object {
        fun <A : Enum<A>> of(vararg aspects: A): PayloadSet<A> {
            require(aspects.isNotEmpty()) { "A payload must carry at least one changed aspect" }
            return PayloadSet(aspects.toSet())
        }

        fun <A : Enum<A>> of(aspects: Collection<A>): PayloadSet<A> {
            require(aspects.isNotEmpty()) { "A payload must carry at least one changed aspect" }
            return PayloadSet(aspects.toSet())
        }

        fun <A : Enum<A>> merge(payloads: List<Any>, aspectType: Class<A>): PayloadSet<A>? {
            val merged = LinkedHashSet<A>()
            payloads.filterIsInstance<PayloadSet<*>>().forEach { payload ->
                merged.addMatching(payload, aspectType)
            }
            return if (merged.isEmpty()) null else PayloadSet(merged)
        }

        private fun <A : Enum<A>> MutableSet<A>.addMatching(
            payload: PayloadSet<*>,
            aspectType: Class<A>,
        ) {
            payload.aspects
                .filter(aspectType::isInstance)
                .mapNotNullTo(this) { aspect -> aspectType.cast(aspect) }
        }
    }
}

abstract class BaseListAdapter<T : Any, VB : ViewBinding, CB : AdapterCallback<T>>(
    protected val callback: CB,
    diff: DiffUtil.ItemCallback<T>,
) : ListAdapter<T, BaseViewHolder<T, VB, CB>>(diff) {

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
