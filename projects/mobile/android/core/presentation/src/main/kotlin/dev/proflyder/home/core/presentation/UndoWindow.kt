package dev.proflyder.home.core.presentation

import android.view.View
import com.google.android.material.snackbar.Snackbar
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.ui.NoticeAction
import dev.proflyder.home.core.ui.Notices
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate

class UndoWindow<T : Any>(scope: CoroutineScope, private val duration: Duration = DEFAULT_UNDO_WINDOW) {
    private val held = MutableStateFlow<T?>(null)
    private val timer = LatestLauncher(scope)

    val pending: StateFlow<T?> = held.asStateFlow()

    fun open(value: T) {
        held.value = value
        timer.launch {
            delay(duration)
            held.value = null
        }
    }

    fun take(): T? {
        timer.cancel()
        return held.getAndUpdate { null }
    }
}

val DEFAULT_UNDO_WINDOW: Duration = 6.seconds

class UndoNotice(private val anchor: () -> View, private val onUndo: () -> Unit) {
    private var shown: Snackbar? = null
    private var message: CharSequence? = null

    fun render(next: CharSequence?) {
        if (next == message && (next == null || shown != null)) return
        clear()
        message = next
        if (next == null) return
        val view = anchor()
        val action = NoticeAction(view.context.getString(DesignR.string.ds_action_undo)) { onUndo() }
        shown = Notices.show(view, next, long = true, indefinite = true, action = action)
    }

    fun clear() {
        shown?.dismiss()
        shown = null
        message = null
    }
}
