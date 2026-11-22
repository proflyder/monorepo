package dev.proflyder.home.feature.scenarios.presentation.editor

import android.os.Bundle
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.designsystem.OptionsSheet
import dev.proflyder.home.core.designsystem.SheetOption
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

internal class ScenarioEditorMenus(
    private val fragment: Fragment,
    private val dispatch: (ScenarioEditorIntent) -> Unit,
) {
    private val manager get() = fragment.childFragmentManager

    fun attach() {
        manager.setFragmentResultListener(MENU_REQUEST, fragment.viewLifecycleOwner) { _, result -> onMenuResult(result) }
        manager.setFragmentResultListener(ACTION_REQUEST, fragment.viewLifecycleOwner) { _, result -> onActionResult(result) }
    }

    fun open(menu: EditorMenu) {
        val options = menu.choices.map { SheetOption(it.name, fragment.getString(it.label), it.icon) }
        OptionsSheet.show(manager, MENU_REQUEST, fragment.getString(menu.title), options)
    }

    fun renderActions(prompt: ScenarioPrompt.ChooseAction?) {
        if (prompt == null) return OptionsSheet.dismiss(manager, ACTION_REQUEST)
        val options = prompt.actions.map { wire -> SheetOption(wire, ScenarioFormat.actionLabel(fragment.requireContext(), wire), actionIcon(wire)) }
        OptionsSheet.show(manager, ACTION_REQUEST, fragment.getString(R.string.scenarios_action_dialog_title), options)
    }

    private fun onMenuResult(result: Bundle) {
        val key = OptionsSheet.chosenKey(result) ?: return
        EditorMenuChoice.entries.firstOrNull { it.name == key }?.let { dispatch(it.intent) }
    }

    private fun onActionResult(result: Bundle) {
        dispatch(OptionsSheet.chosenKey(result)?.let(ScenarioEditorIntent::ActionChosen) ?: ScenarioEditorIntent.DismissPrompt)
    }

    private fun actionIcon(wire: String): Int =
        if (wire == ScenarioPromptFlow.ACTION_SPEAK) R.drawable.scenarios_ic_speak else R.drawable.scenarios_ic_tune

    private companion object {
        const val MENU_REQUEST = "scenario_editor_menu"
        const val ACTION_REQUEST = "scenario_editor_action"
    }
}
