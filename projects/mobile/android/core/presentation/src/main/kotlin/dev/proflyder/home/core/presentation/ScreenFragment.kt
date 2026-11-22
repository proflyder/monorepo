package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.designsystem.PillSegments
import android.os.Bundle
import android.view.LayoutInflater
import android.view.ViewGroup
import android.widget.CompoundButton
import android.widget.EditText
import androidx.fragment.app.Fragment
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.navigation.open
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.injectedViewModels

abstract class ScreenFragment<VB : ViewBinding, S : Any, I : Any, E : Any>(
    inflateBinding: (LayoutInflater, ViewGroup?, Boolean) -> VB,
) : BaseFragment<VB>(inflateBinding) {
    protected abstract val viewModel: StateViewModel<S, I, E>

    final override fun onBindingCreated(binding: VB, savedInstanceState: Bundle?) {
        onScreenCreated(binding, savedInstanceState)
        viewModel.state.observe { render(it) }
        viewModel.events.observe { onEvent(it) }
    }

    protected open fun onScreenCreated(binding: VB, savedInstanceState: Bundle?) = Unit

    protected abstract fun render(state: S)

    protected open fun onEvent(event: E) = Unit

    protected fun dispatch(intent: I) {
        viewModel.dispatch(intent)
    }

    protected fun navigate(route: Route) {
        requireNavigator().open(route)
    }

    protected fun bindSwitch(view: CompoundButton, toIntent: (Boolean) -> I): SwitchBinder =
        SwitchBinder(view) { checked -> dispatch(toIntent(checked)) }

    protected fun <T> bindPills(view: PillSegments, values: List<T>, toIntent: (T) -> I): PillSegmentsBinder<T> =
        PillSegmentsBinder(view, values) { value -> dispatch(toIntent(value)) }

    protected fun bindText(view: EditText, toIntent: (String) -> I): TextBinder =
        TextBinder(view) { text -> dispatch(toIntent(text)) }
}

inline fun <reified VM : ViewModel> Fragment.screenViewModel(): Lazy<VM> =
    injectedViewModels()
