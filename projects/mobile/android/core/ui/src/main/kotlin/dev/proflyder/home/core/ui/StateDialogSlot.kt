package dev.proflyder.home.core.ui

import android.app.Dialog

class StateDialogSlot {
    private var key: Any? = null
    private var dialog: Dialog? = null

    fun isShowing(key: Any): Boolean = this.key == key && dialog?.isShowing == true

    fun show(key: Any, onUserClose: () -> Unit, create: () -> Dialog) {
        if (isShowing(key)) return
        clear()
        this.key = key
        dialog = create().also { it.setOnCancelListener { onUserClose() } }.also(Dialog::show)
    }

    fun clear() {
        dialog?.setOnCancelListener(null)
        dialog?.dismiss()
        dialog = null
        key = null
    }
}
