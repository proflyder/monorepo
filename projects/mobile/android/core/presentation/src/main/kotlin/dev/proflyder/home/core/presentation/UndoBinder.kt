package dev.proflyder.home.core.presentation

import android.view.View
import dev.proflyder.home.core.model.Keyed

class UndoBinder<T : Keyed>(
    anchor: () -> View,
    private val message: () -> CharSequence,
    dispatch: (ListItemIntent) -> Unit,
) {
    private val notice = UndoNotice(anchor) { dispatch(ListItemIntent.Undo) }

    fun render(pending: T?) {
        notice.render(pending?.let { message() })
    }

    fun clear() = notice.clear()
}
