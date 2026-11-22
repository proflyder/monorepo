package dev.proflyder.home.core.presentation

import android.widget.CompoundButton
import android.widget.EditText
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.proflyder.home.core.designsystem.PillSegments
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

interface IntentSink<I : Any> {
    fun dispatch(intent: I)
}

fun <I : Any> IntentSink<I>.bindSwitch(view: CompoundButton, toIntent: (Boolean) -> I): SwitchBinder =
    SwitchBinder(view) { checked -> dispatch(toIntent(checked)) }

fun <I : Any, T> IntentSink<I>.bindPills(view: PillSegments, values: List<T>, toIntent: (T) -> I): PillSegmentsBinder<T> =
    PillSegmentsBinder(view, values) { value -> dispatch(toIntent(value)) }

fun <I : Any> IntentSink<I>.bindText(view: EditText, toIntent: (String) -> I): TextBinder =
    TextBinder(view) { text -> dispatch(toIntent(text)) }

internal fun <S : Any, E : Any> LifecycleOwner.connect(
    viewModel: StateViewModel<S, *, E>,
    render: (S) -> Unit,
    onEvent: (E) -> Unit,
) {
    collectStarted(viewModel.state, render)
    collectStarted(viewModel.events, onEvent)
}

private fun <T> LifecycleOwner.collectStarted(flow: Flow<T>, action: (T) -> Unit) {
    lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { flow.collect(action) } }
}
