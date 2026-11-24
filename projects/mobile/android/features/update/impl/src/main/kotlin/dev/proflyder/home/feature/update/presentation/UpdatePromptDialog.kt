package dev.proflyder.home.feature.update.presentation

import android.content.Context
import android.view.LayoutInflater
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateState
import dev.proflyder.home.feature.update.impl.databinding.DialogUpdatePromptBinding

internal class UpdatePromptDialog(
    context: Context,
    updates: UpdateCenter,
) {
    private val binding = DialogUpdatePromptBinding.inflate(LayoutInflater.from(context))
    private val content = UpdatePromptContent(context, binding, updates)
    private val dialog = MaterialAlertDialogBuilder(context)
        .setView(binding.root)
        .create()

    fun render(state: UpdateState) {
        if (!state.promptVisible) {
            dismiss()
            return
        }
        content.render(state)
        if (!dialog.isShowing) runCatching { dialog.show() }
    }

    fun dismiss() {
        if (dialog.isShowing) dialog.dismiss()
    }
}
