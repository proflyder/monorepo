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

    final override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        inflateBinding(inflater, container, false).also { _binding = it }.root

    final override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        onSheetCreated(binding, savedInstanceState)
        viewLifecycleOwner.connect(viewModel, ::render, ::onEvent)
    }

    final override fun onDestroyView() {
        onSheetDestroyed()
        _binding = null
        super.onDestroyView()
    }

    protected open fun onSheetCreated(binding: VB, savedInstanceState: Bundle?) = Unit

    protected open fun onSheetDestroyed() = Unit

    protected abstract fun render(state: S)

    protected open fun onEvent(event: E) = Unit

    final override fun dispatch(intent: I) {
        viewModel.dispatch(intent)
    }
}
