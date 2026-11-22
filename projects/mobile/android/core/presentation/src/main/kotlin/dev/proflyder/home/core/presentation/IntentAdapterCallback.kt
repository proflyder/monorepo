package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.ui.AdapterCallback

class IntentAdapterCallback<T, I : Any>(
    private val dispatch: (I) -> Unit,
    private val onClick: (T) -> I?,
    private val onLongClick: (T) -> I? = { null },
) : AdapterCallback<T> {
    override fun onItemClick(item: T) {
        onClick(item)?.let(dispatch)
    }

    override fun onItemLongClick(item: T) {
        onLongClick(item)?.let(dispatch)
    }
}
