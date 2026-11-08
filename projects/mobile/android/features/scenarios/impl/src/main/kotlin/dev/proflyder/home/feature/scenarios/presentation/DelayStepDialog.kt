package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ScenarioLimits
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioDelayBinding
import java.util.Locale

/** Delay step: seconds or minutes, always stored as seconds within 1..3600. */
internal class DelayStepDialog(
    context: Context,
    initialSeconds: Int,
    private val onSave: (Int) -> Unit,
) {
    private val views = DialogScenarioDelayBinding.inflate(LayoutInflater.from(context))
    private var minutes = false

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_step_delay_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save, null)
        .create()

    init {
        val seconds = initialSeconds.coerceIn(
            ScenarioLimits.MIN_DELAY_SECONDS,
            ScenarioLimits.MAX_DELAY_SECONDS,
        )
        if (seconds % 60 == 0) {
            minutes = true
            views.delayInput.setText(String.format(Locale.ROOT, "%d", seconds / 60))
        } else {
            views.delayInput.setText(String.format(Locale.ROOT, "%d", seconds))
        }
        views.delayUnitGroup.check(if (minutes) R.id.delay_unit_minutes else R.id.delay_unit_seconds)
        views.delayUnitGroup.setOnCheckedStateChangeListener { _, checked ->
            minutes = checked.firstOrNull() == R.id.delay_unit_minutes
            renderPositive()
        }
        views.delayInput.doAfterTextChanged { renderPositive() }
    }

    fun show() {
        dialog.setOnShowListener { renderPositive() }
        dialog.show()
    }

    private fun seconds(): Int? {
        val value = views.delayInput.text?.toString()?.toIntOrNull() ?: return null
        val result = if (minutes) value * 60 else value
        return result.takeIf { it in ScenarioLimits.MIN_DELAY_SECONDS..ScenarioLimits.MAX_DELAY_SECONDS }
    }

    private fun renderPositive() {
        val button = runCatching { dialog.getButton(DialogInterface.BUTTON_POSITIVE) }.getOrNull() ?: return
        button.isEnabled = seconds() != null
        button.setOnClickListener {
            val value = seconds() ?: return@setOnClickListener
            onSave(value)
            dialog.dismiss()
        }
    }
}
