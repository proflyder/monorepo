package dev.proflyder.home.core.presentation

import android.annotation.SuppressLint
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.ui.PayloadSet

class KeyedDiff<T : Any>(
    private val key: (T) -> Any,
    private val payload: (T, T) -> Any? = { _, _ -> null },
) : DiffUtil.ItemCallback<T>() {
    override fun areItemsTheSame(oldItem: T, newItem: T): Boolean = key(oldItem) == key(newItem)

    @SuppressLint("DiffUtilEquals")
    override fun areContentsTheSame(oldItem: T, newItem: T): Boolean = oldItem == newItem

    override fun getChangePayload(oldItem: T, newItem: T): Any? = payload(oldItem, newItem)
}

fun <T, A : Enum<A>> aspectPayload(aspects: Map<A, (T) -> Any?>): (T, T) -> Any? = { old, new ->
    val changed = aspects.filter { (_, read) -> read(old) != read(new) }.keys
    when {
        changed.isEmpty() -> null
        else -> PayloadSet.of(changed)
    }
}
