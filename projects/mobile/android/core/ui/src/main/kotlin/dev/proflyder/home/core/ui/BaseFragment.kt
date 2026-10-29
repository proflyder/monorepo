package dev.proflyder.home.core.ui

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.annotation.StringRes
import android.content.DialogInterface
import androidx.fragment.app.Fragment
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.color.MaterialColors
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.snackbar.Snackbar
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

/**
 * Base for screens backed by a generated ViewBinding.
 *
 * The binding exists strictly between [onCreateView] and [onDestroyView]; it is
 * released there so the Fragment never leaks its view hierarchy. Subclasses wire
 * the view in [onBindingCreated] and observe state with [observe].
 *
 * ```
 * class FooFragment : BaseFragment<FragmentFooBinding>(FragmentFooBinding.Companion::inflate)
 * ```
 */
abstract class BaseFragment<VB : ViewBinding>(
    private val inflateBinding: (LayoutInflater, ViewGroup?, Boolean) -> VB,
) : Fragment() {
    private var _binding: VB? = null

    /** Valid between onViewCreated and onDestroyView. */
    protected val binding: VB
        get() = checkNotNull(_binding) { "${javaClass.simpleName}.binding accessed outside the view lifecycle" }

    final override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View = inflateBinding(inflater, container, false).also { _binding = it }.root

    final override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        onBindingCreated(binding, savedInstanceState)
    }

    final override fun onDestroyView() {
        onBindingDestroyed()
        _binding = null
        super.onDestroyView()
    }

    /** Set up listeners, adapters and state collection here. */
    protected abstract fun onBindingCreated(binding: VB, savedInstanceState: Bundle?)

    /** Release anything that references views (adapters on RecyclerViews, etc.). */
    protected open fun onBindingDestroyed() = Unit

    /** Runs [block] while the view is at least [minState]; cancelled when it drops below, restarted on return. */
    protected fun repeatOnView(
        minState: Lifecycle.State = Lifecycle.State.STARTED,
        block: suspend CoroutineScope.() -> Unit,
    ) {
        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(minState, block)
        }
    }

    /** Collects [this] while the view is at least [minState]. */
    protected fun <T> Flow<T>.observe(
        minState: Lifecycle.State = Lifecycle.State.STARTED,
        action: suspend (T) -> Unit,
    ) {
        val source = this
        repeatOnView(minState) { source.collect { action(it) } }
    }

    protected fun showMessage(@StringRes message: Int) {
        Snackbar.make(binding.root, message, Snackbar.LENGTH_SHORT).show()
    }

    protected fun showFailure(failure: RemoteFailure) {
        Snackbar.make(binding.root, failure.userMessage(requireContext()), Snackbar.LENGTH_LONG).show()
    }

    /**
     * Material confirmation dialog; [onConfirm] runs only on the positive button.
     * [destructive] tints the positive button with the error color.
     */
    protected fun confirm(
        @StringRes message: Int,
        @StringRes positive: Int,
        @StringRes negative: Int,
        destructive: Boolean = true,
        onConfirm: () -> Unit,
    ) = confirm(getString(message), positive, negative, destructive, onConfirm)

    protected fun confirm(
        message: CharSequence,
        @StringRes positive: Int,
        @StringRes negative: Int,
        destructive: Boolean = true,
        onConfirm: () -> Unit,
    ) {
        val dialog = MaterialAlertDialogBuilder(requireContext())
            .setMessage(message)
            .setNegativeButton(negative, null)
            .setPositiveButton(positive) { _, _ -> onConfirm() }
            .create()
        if (destructive) {
            dialog.setOnShowListener {
                val error = MaterialColors.getColor(binding.root, androidx.appcompat.R.attr.colorError)
                dialog.getButton(DialogInterface.BUTTON_POSITIVE).setTextColor(error)
            }
        }
        dialog.show()
    }
}
