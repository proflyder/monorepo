package dev.proflyder.home.core.designsystem

import android.app.Dialog
import android.content.DialogInterface
import android.os.Bundle
import android.view.View
import androidx.annotation.StringRes
import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.DialogFragment
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

interface EditorSession<T : Any> {
    val value: StateFlow<T?>

    fun canCommit(value: T): Boolean

    fun commit()

    fun cancel()
}

abstract class EditorDialogFragment<T : Any> : DialogFragment() {
    protected abstract val session: EditorSession<T>

    @get:StringRes
    protected abstract val titleRes: Int

    protected abstract fun createContent(): View

    protected abstract fun render(value: T)

    protected open fun onShown() = Unit

    override fun onCreateDialog(savedInstanceState: Bundle?): Dialog {
        val dialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(titleRes)
            .setView(createContent())
            .setNegativeButton(android.R.string.cancel) { _, _ -> session.cancel() }
            .setPositiveButton(R.string.ds_action_save, null)
            .create()
        dialog.setOnShowListener { onDialogShown(dialog) }
        return dialog
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { session.value.collect(::onValue) } }
    }

    override fun onCancel(dialog: DialogInterface) {
        super.onCancel(dialog)
        session.cancel()
    }

    private fun onDialogShown(dialog: AlertDialog) {
        dialog.getButton(DialogInterface.BUTTON_POSITIVE).setOnClickListener { session.commit() }
        session.value.value?.let(::onValue)
        onShown()
    }

    private fun onValue(value: T?) {
        if (value == null) return dismissAllowingStateLoss()
        render(value)
        (dialog as? AlertDialog)?.getButton(DialogInterface.BUTTON_POSITIVE)?.isEnabled = session.canCommit(value)
    }
}
