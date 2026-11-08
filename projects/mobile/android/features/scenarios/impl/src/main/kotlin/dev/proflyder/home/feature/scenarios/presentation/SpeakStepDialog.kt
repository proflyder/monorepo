package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.text.SpeakText
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioSpeakBinding

/** Speak step text with the shared 500-character normalization rules. */
internal class SpeakStepDialog(
    context: Context,
    initial: String,
    private val onSave: (String) -> Unit,
) {
    private val views = DialogScenarioSpeakBinding.inflate(LayoutInflater.from(context))

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_step_speak_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save, null)
        .create()

    init {
        views.speakInput.setText(initial)
        views.speakInput.doAfterTextChanged {
            renderCounter()
            renderPositive()
        }
        renderCounter()
    }

    fun show() {
        dialog.setOnShowListener {
            renderPositive()
            views.speakInput.requestFocus()
        }
        dialog.show()
    }

    private fun renderCounter() {
        views.speakCounter.text = views.root.resources.getQuantityString(
            R.plurals.scenarios_speak_counter,
            SpeakText.remaining(views.speakInput.text?.toString().orEmpty()),
            SpeakText.remaining(views.speakInput.text?.toString().orEmpty()),
        )
    }

    private fun renderPositive() {
        val button = runCatching { dialog.getButton(DialogInterface.BUTTON_POSITIVE) }.getOrNull() ?: return
        button.isEnabled = speakable() != null
        button.setOnClickListener {
            val text = speakable() ?: return@setOnClickListener
            onSave(text)
            dialog.dismiss()
        }
    }

    private fun speakable(): String? = SpeakText.normalize(views.speakInput.text?.toString().orEmpty())
}
