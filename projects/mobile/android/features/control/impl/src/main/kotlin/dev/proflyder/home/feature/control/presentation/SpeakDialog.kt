package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.feature.control.SpeakText
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.DialogSpeakBinding

internal class SpeakDialog(
    context: Context,
    private val onSpeak: (String) -> Unit,
) {
    private val views = DialogSpeakBinding.inflate(LayoutInflater.from(context))
    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.control_speak_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.control_speak_send, null)
        .create()

    init {
        dialog.setOnShowListener { installSendButton() }
    }

    private fun installSendButton() {
        val send = dialog.getButton(DialogInterface.BUTTON_POSITIVE)
        send.isEnabled = speakable() != null
        send.setOnClickListener { sendText() }
        views.speakInput.doAfterTextChanged {
            send.isEnabled = speakable() != null
        }
    }

    private fun sendText() {
        val text = speakable() ?: return
        onSpeak(text)
        dialog.dismiss()
    }

    fun show() {
        dialog.show()
        views.speakInput.requestFocus()
    }

    fun dismiss() = dialog.dismiss()

    private fun speakable(): String? = SpeakText.normalize(views.speakInput.text?.toString().orEmpty())
}
