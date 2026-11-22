package dev.proflyder.home.feature.control.presentation

import android.view.View
import android.view.ViewGroup
import dev.proflyder.home.feature.control.impl.R

internal fun <B : Any, T> ViewGroup.bindChildren(
    items: List<T>,
    create: (ViewGroup) -> Pair<View, B>,
    bind: (B, T) -> Unit,
) {
    while (childCount > items.size) removeViewAt(childCount - 1)
    while (childCount < items.size) addView(create(this).attached())
    items.forEachIndexed { index, item -> bind(binderAt<B>(index), item) }
}

private fun <B : Any> Pair<View, B>.attached(): View = first.also { it.setTag(R.id.control_row_binder, second) }

@Suppress("UNCHECKED_CAST")
private fun <B : Any> ViewGroup.binderAt(index: Int): B = getChildAt(index).getTag(R.id.control_row_binder) as B
