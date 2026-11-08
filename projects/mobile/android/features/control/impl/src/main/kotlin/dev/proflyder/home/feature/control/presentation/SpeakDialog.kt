package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.feature.control.SpeakText
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.DialogSpeakBinding

/**
 * The "say on the station" dialog: a multiline text with a 500-character
 * counter. "Say" stays disabled until the text is speakable; the text itself
 * is never logged, only put into the queued command.
 */
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
        // The positive button is installed by hand so an invalid text cannot
        // dismiss the dialog and "succeed" silently.
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
