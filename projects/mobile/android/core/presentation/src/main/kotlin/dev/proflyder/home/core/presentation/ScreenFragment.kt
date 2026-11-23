package dev.proflyder.home.core.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.ViewGroup
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
) : BaseFragment<VB>(inflateBinding), IntentSink<I>, TrackedScreen {
    protected abstract val viewModel: StateViewModel<S, I, E>

    override val screenName: String get() = screenNameOf(this)

    final override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        deliverEvents(viewModel.screenEvents, ::onEvent)
        onScreenCreate(savedInstanceState)
    }

    final override fun onBindingCreated(binding: VB, savedInstanceState: Bundle?) {
        onViewReady(binding, savedInstanceState)
        bindScreenView(viewModel, ::render)
    }

    protected open fun onScreenCreate(savedInstanceState: Bundle?) = Unit

    protected open fun onViewReady(binding: VB, savedInstanceState: Bundle?) = Unit

    protected open fun render(state: S) = Unit

    protected fun <T> render(selector: (S) -> T, block: (T) -> Unit) {
        viewLifecycleOwner.renderSlice(viewModel.state, selector, block)
    }

    protected open fun onEvent(event: E) = Unit

    protected fun <T : Any> onResult(key: ResultKey<T>, toIntent: (T) -> I?) {
        childFragmentManager.listenResult(key, this) { value -> toIntent(value)?.let(::dispatch) }
    }

    final override fun dispatch(intent: I) {
        viewModel.dispatch(intent)
    }

    protected fun navigate(route: Route) {
        requireNavigator().open(route)
    }
}

inline fun <reified VM : ViewModel> Fragment.screenViewModel(): Lazy<VM> =
    injectedViewModels()
