package dev.proflyder.home.feature.scenarios.presentation.dialog

import android.view.View
import android.widget.EditText
import androidx.core.widget.doAfterTextChanged
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioDelayBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioEntityConditionBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioSpeakBinding
import dev.proflyder.home.feature.scenarios.presentation.editor.DelayForm
import dev.proflyder.home.feature.scenarios.presentation.editor.EntityConditionForm
import dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorIntent
import dev.proflyder.home.feature.scenarios.presentation.editor.SpeakForm
import dev.proflyder.home.feature.scenarios.presentation.titleOf

internal class SpeakFormDialog : ScenarioFormDialog<SpeakForm>(SpeakForm::class) {
    private lateinit var views: DialogScenarioSpeakBinding

    override val titleRes: Int = R.string.scenarios_step_speak_title

    override fun createContent(): View {
        views = DialogScenarioSpeakBinding.inflate(layoutInflater)
        views.speakInput.doAfterTextChanged { text -> edit { it.copy(text = text?.toString().orEmpty()) } }
        return views.root
    }

    override fun render(value: SpeakForm) {
        views.speakInput.showText(value.text)
        views.speakCounter.text = resources.getQuantityString(R.plurals.scenarios_speak_counter, value.remaining, value.remaining)
    }

    override fun onShown() {
        views.speakInput.requestFocus()
    }
}

internal class DelayFormDialog : ScenarioFormDialog<DelayForm>(DelayForm::class) {
    private lateinit var views: DialogScenarioDelayBinding
    private lateinit var seconds: SwitchBinder
    private lateinit var minutes: SwitchBinder

    override val titleRes: Int = R.string.scenarios_step_delay_title

    override fun createContent(): View {
        views = DialogScenarioDelayBinding.inflate(layoutInflater)
        views.delayInput.doAfterTextChanged { text -> edit { it.copy(amount = text?.toString().orEmpty()) } }
        seconds = SwitchBinder(views.delayUnitSeconds) { checked -> if (checked) edit { it.copy(minutes = false) } }
        minutes = SwitchBinder(views.delayUnitMinutes) { checked -> if (checked) edit { it.copy(minutes = true) } }
        return views.root
    }

    override fun render(value: DelayForm) {
        views.delayInput.showText(value.amount)
        seconds.render(!value.minutes)
        minutes.render(value.minutes)
    }
}

internal class EntityConditionFormDialog : ScenarioFormDialog<EntityConditionForm>(EntityConditionForm::class) {
    private lateinit var views: DialogScenarioEntityConditionBinding
    private lateinit var on: SwitchBinder
    private lateinit var off: SwitchBinder

    override val titleRes: Int = R.string.scenarios_condition_dialog_title

    override fun createContent(): View {
        views = DialogScenarioEntityConditionBinding.inflate(layoutInflater)
        views.conditionEntityButton.setOnClickListener { dispatch(ScenarioEditorIntent.PickConditionEntity) }
        views.conditionStateInput.doAfterTextChanged { text -> edit { it.copy(state = text?.toString().orEmpty()) } }
        on = SwitchBinder(views.conditionStateOn) { checked -> if (checked) edit { it.copy(state = EntityConditionForm.STATE_ON) } }
        off = SwitchBinder(views.conditionStateOff) { checked -> if (checked) edit { it.copy(state = EntityConditionForm.STATE_OFF) } }
        return views.root
    }

    override fun render(value: EntityConditionForm) {
        views.conditionStateInput.showText(value.state)
        on.render(value.state == EntityConditionForm.STATE_ON)
        off.render(value.state == EntityConditionForm.STATE_OFF)
        val id = value.entityId
        views.conditionEntityButton.text = when {
            id.isBlank() -> getString(R.string.scenarios_condition_choose_entity)
            else -> editorState.catalog.entries.titleOf(id)?.let { "$it ($id)" } ?: id
        }
    }
}

internal fun EditText.showText(value: String) {
    if (text?.toString() != value) setText(value)
}
