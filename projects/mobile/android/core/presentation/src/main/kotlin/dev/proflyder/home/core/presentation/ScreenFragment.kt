package dev.proflyder.home.core.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.ViewGroup
import android.widget.CompoundButton
import android.widget.EditText
import androidx.fragment.app.Fragment
import androidx.fragment.app.viewModels
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.viewModelFactory

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

    protected fun bindSwitch(view: CompoundButton, toIntent: (Boolean) -> I): SwitchBinder =
        SwitchBinder(view) { checked -> dispatch(toIntent(checked)) }

    protected fun bindText(view: EditText, toIntent: (String) -> I): TextBinder =
        TextBinder(view) { text -> dispatch(toIntent(text)) }
}

inline fun <reified VM : ViewModel> Fragment.screenViewModel(): Lazy<VM> =
    viewModels { viewModelFactory(VM::class.java) }
