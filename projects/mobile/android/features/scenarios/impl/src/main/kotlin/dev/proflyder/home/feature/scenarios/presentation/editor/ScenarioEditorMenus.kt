package dev.proflyder.home.feature.scenarios.presentation.editor

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.presentation.OptionsResult
import dev.proflyder.home.core.presentation.OptionsSheet
import dev.proflyder.home.core.presentation.SheetChoice
import dev.proflyder.home.core.presentation.SheetOption
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.presentation.ScenarioFormat

internal enum class EditorMenuChoice(
    @StringRes val label: Int,
    @DrawableRes val icon: Int,
    val intent: ScenarioEditorIntent,
) {
    TIME_TRIGGER(R.string.scenarios_editor_add_trigger_time, R.drawable.scenarios_ic_schedule, ScenarioEditorIntent.AddTimeTrigger),
    ENTITY_CONDITION(R.string.scenarios_editor_add_condition_entity, R.drawable.scenarios_ic_devices, ScenarioEditorIntent.AddEntityCondition),
    TIME_WINDOW(R.string.scenarios_editor_add_condition_window, R.drawable.scenarios_ic_schedule, ScenarioEditorIntent.AddTimeWindow),
    CATALOG_STEP(R.string.scenarios_editor_add_action, R.drawable.scenarios_ic_tune, ScenarioEditorIntent.AddCatalogStep),
    SPEAK_STEP(R.string.scenarios_editor_add_speak, R.drawable.scenarios_ic_speak, ScenarioEditorIntent.AddSpeakStep),
    DELAY_STEP(R.string.scenarios_editor_add_delay, R.drawable.scenarios_ic_timer, ScenarioEditorIntent.AddDelayStep),
}

internal enum class EditorMenu(@StringRes val title: Int, val choices: List<EditorMenuChoice>) {
    TRIGGER(R.string.scenarios_editor_add_trigger_title, listOf(EditorMenuChoice.TIME_TRIGGER)),
    CONDITION(
        R.string.scenarios_editor_add_condition_title,
        listOf(EditorMenuChoice.ENTITY_CONDITION, EditorMenuChoice.TIME_WINDOW),
    ),
    STEP(
        R.string.scenarios_editor_add_step_title,
        listOf(EditorMenuChoice.CATALOG_STEP, EditorMenuChoice.SPEAK_STEP, EditorMenuChoice.DELAY_STEP),
    ),
}

internal object EditorMenuChosen : OptionsResult()

internal object EditorActionChosen : OptionsResult()

internal fun menuIntent(choice: SheetChoice): ScenarioEditorIntent? =
    EditorMenuChoice.entries.firstOrNull { it.name == choice.key }?.intent

internal fun actionIntent(choice: SheetChoice): ScenarioEditorIntent =
    choice.key?.let(ScenarioEditorIntent::ActionChosen) ?: ScenarioEditorIntent.DismissPrompt

internal class ScenarioEditorMenus(private val fragment: Fragment) {
    private val manager get() = fragment.childFragmentManager

    fun open(menu: EditorMenu) {
        val options = menu.choices.map { SheetOption(it.name, fragment.getString(it.label), it.icon) }
        OptionsSheet.show(manager, EditorMenuChosen, fragment.getString(menu.title), options)
    }

    fun renderActions(prompt: ScenarioPrompt.ChooseAction?) {
        if (prompt == null) return OptionsSheet.dismiss(manager, EditorActionChosen)
        val options = prompt.actions.map { wire -> SheetOption(wire, ScenarioFormat.actionLabel(fragment.requireContext(), wire), actionIcon(wire)) }
        OptionsSheet.show(manager, EditorActionChosen, fragment.getString(R.string.scenarios_action_dialog_title), options)
    }

    private fun actionIcon(wire: String): Int =
        if (wire == ScenarioPromptFlow.ACTION_SPEAK) R.drawable.scenarios_ic_speak else R.drawable.scenarios_ic_tune
}
