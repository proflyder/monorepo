package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioEntityConditionBinding

/**
 * One entity-state condition. The entity is picked from the live catalog; the
 * value can be the common `on`/`off` or any exact Home Assistant state string.
 */
internal class EntityConditionDialog(
    context: Context,
    initialEntityId: String,
    initialState: String,
    entryTitle: (String) -> String?,
    private val onPickEntity: (onPicked: (String) -> Unit) -> Unit,
    private val onSave: (entityId: String, state: String) -> Unit,
) {
    private val views = DialogScenarioEntityConditionBinding.inflate(LayoutInflater.from(context))
    private var entityId = initialEntityId
    private var bindingState = false

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_condition_dialog_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save, null)
        .create()

    init {
        views.conditionStateInput.setText(initialState)
        views.conditionEntityButton.setOnClickListener {
            onPickEntity { picked ->
                entityId = picked
                renderEntity(entryTitle)
            }
        }
        views.conditionStateGroup.setOnCheckedStateChangeListener { _, checked ->
            if (bindingState || checked.isEmpty()) return@setOnCheckedStateChangeListener
            views.conditionStateInput.setText(
                if (checked.first() == R.id.condition_state_on) "on" else "off",
            )
        }
        views.conditionStateInput.doAfterTextChanged { onStateTextChanged() }
        renderEntity(entryTitle)
    }

    private fun onStateTextChanged() {
        bindingState = true
        views.conditionStateGroup.clearCheck()
        bindingState = false
        renderPositive()
    }

    fun show() {
        dialog.setOnShowListener { renderPositive() }
        dialog.show()
    }

    private fun renderEntity(entryTitle: (String) -> String?) {
        views.conditionEntityButton.text =
            entityId.ifBlank { views.root.context.getString(R.string.scenarios_condition_choose_entity) }
                .let { id -> entryTitle(id)?.let { "$it ($id)" } ?: id }
        renderPositive()
    }

    private fun renderPositive() {
        val button = runCatching { dialog.getButton(DialogInterface.BUTTON_POSITIVE) }.getOrNull() ?: return
        button.isEnabled = entityId.isNotBlank() && views.conditionStateInput.text?.toString()?.isNotBlank() == true
        button.setOnClickListener {
            if (!button.isEnabled) return@setOnClickListener
            onSave(entityId, views.conditionStateInput.text?.toString().orEmpty())
            dialog.dismiss()
        }
    }
}
