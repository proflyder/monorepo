package dev.proflyder.home.feature.scenarios.presentation.editor

import android.app.Dialog

internal class DialogSlot {
    private var key: Any? = null
    private var dialog: Dialog? = null

    fun show(key: Any, onCancel: () -> Unit, create: () -> Dialog) {
        if (this.key == key && dialog?.isShowing == true) return
        clear()
        this.key = key
        dialog = create().also { it.setOnCancelListener { onCancel() } }.also(Dialog::show)
    }

    fun clear() {
        dialog?.setOnCancelListener(null)
        dialog?.dismiss()
        dialog = null
        key = null
    }
}
