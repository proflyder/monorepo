package dev.proflyder.home.core.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import dev.proflyder.home.core.ui.ViewBinding

abstract class ScreenBottomSheet<VB : ViewBinding, S : Any, I : Any, E : Any>(
    private val inflateBinding: (LayoutInflater, ViewGroup?, Boolean) -> VB,
) : BottomSheetDialogFragment(), IntentSink<I>, TrackedScreen {
    private var _binding: VB? = null

    protected val binding: VB
        get() = checkNotNull(_binding) { "${javaClass.simpleName}.binding accessed outside the view lifecycle" }

    protected abstract val viewModel: StateViewModel<S, I, E>

    override val screenName: String get() = screenNameOf(this)

    final override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        deliverEvents(viewModel.screenEvents, ::onEvent)
        onScreenCreate(savedInstanceState)
    }

    final override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        inflateBinding(inflater, container, false).also { _binding = it }.root

    final override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        onViewReady(binding, savedInstanceState)
        bindScreenView(viewModel, ::render)
    }

    final override fun onDestroyView() {
        onViewDestroyed()
        _binding = null
        super.onDestroyView()
    }

    protected open fun onScreenCreate(savedInstanceState: Bundle?) = Unit

    protected open fun onViewReady(binding: VB, savedInstanceState: Bundle?) = Unit

    protected open fun onViewDestroyed() = Unit

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
}
