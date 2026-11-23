package dev.proflyder.home.core.presentation

import android.widget.CompoundButton
import android.widget.EditText
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.designsystem.PillSegments

interface IntentSink<I : Any> {
    fun dispatch(intent: I)
}

fun <I : Any> IntentSink<I>.bindSwitch(view: CompoundButton, toIntent: (Boolean) -> I): SwitchBinder =
    SwitchBinder(view) { checked -> dispatch(toIntent(checked)) }

fun <I : Any, T> IntentSink<I>.bindPills(view: PillSegments, values: List<T>, toIntent: (T) -> I): PillSegmentsBinder<T> =
    PillSegmentsBinder(view, values) { value -> dispatch(toIntent(value)) }

fun <I : Any> IntentSink<I>.bindText(view: EditText, toIntent: (String) -> I): TextBinder =
    TextBinder(view) { text -> dispatch(toIntent(text)) }

internal fun <S : Any> Fragment.bindScreenView(viewModel: StateViewModel<S, *, *>, render: (S) -> Unit) {
    viewLifecycleOwner.reportVisibility(viewModel.visibility)
    viewLifecycleOwner.renderSlice(viewModel.state, { it }, render)
}
