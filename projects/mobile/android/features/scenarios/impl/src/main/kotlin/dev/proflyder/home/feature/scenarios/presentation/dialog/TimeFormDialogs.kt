package dev.proflyder.home.feature.scenarios.presentation.dialog

import android.view.View
import android.widget.CompoundButton
import dev.proflyder.home.core.designsystem.LightParamsEditor
import dev.proflyder.home.core.designsystem.apply
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioTimeTriggerBinding
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioTimeWindowBinding
import dev.proflyder.home.feature.scenarios.presentation.editor.LightForm
import dev.proflyder.home.feature.scenarios.presentation.editor.TimeTriggerForm
import dev.proflyder.home.feature.scenarios.presentation.editor.TimeWindowForm
import java.time.format.DateTimeFormatter
import java.util.Locale

internal class TimeTriggerFormDialog : ScenarioFormDialog<TimeTriggerForm>(TimeTriggerForm::class) {
    private lateinit var views: DialogScenarioTimeTriggerBinding
    private lateinit var repeats: Map<RepeatKind, SwitchBinder>
    private lateinit var days: List<SwitchBinder>

    override val titleRes: Int = R.string.scenarios_trigger_dialog_title

    override fun createContent(): View {
        views = DialogScenarioTimeTriggerBinding.inflate(layoutInflater)
        views.triggerTime.setOnClickListener { pickTime() }
        views.triggerDate.setOnClickListener { pickDate() }
        repeats = repeatChips().mapValues { (kind, chip) -> SwitchBinder(chip) { checked -> if (checked) edit { it.copy(repeat = kind) } } }
        days = dayBinders(dayChips()) { day, checked -> edit { it.toggledDay(day, checked) } }
        return views.root
    }

    override fun render(value: TimeTriggerForm) = with(views) {
        triggerTimezone.text = value.zone
        triggerTime.text = value.at.formatted()
        repeats.forEach { (kind, binder) -> binder.render(kind == value.repeat) }
        triggerDate.setVisible(value.repeat == RepeatKind.ONCE)
        triggerDate.text = value.onceDate.format(DateTimeFormatter.ofPattern(DATE_PATTERN, Locale.getDefault()))
        triggerDaysLabel.setVisible(value.repeat == RepeatKind.CUSTOM)
        triggerDaysGroup.setVisible(value.repeat == RepeatKind.CUSTOM)
        days.forEachIndexed { index, binder -> binder.render(index + 1 in value.weekdays) }
    }

    private fun pickTime() {
        val at = session.value.value?.at ?: return
        parentFragmentManager.showTimePicker(at.hour, at.minute) { hour, minute -> edit { it.copy(at = WallTime(hour, minute)) } }
    }

    private fun pickDate() {
        val form = session.value.value ?: return
        parentFragmentManager.showDatePicker(form.onceDate, form.today) { date -> edit { it.copy(onceDate = date) } }
    }

    private fun repeatChips(): Map<RepeatKind, CompoundButton> = with(views) {
        mapOf(
            RepeatKind.ONCE to triggerRepeatOnce,
            RepeatKind.DAILY to triggerRepeatDaily,
            RepeatKind.WEEKDAYS to triggerRepeatWeekdays,
            RepeatKind.WEEKENDS to triggerRepeatWeekends,
            RepeatKind.CUSTOM to triggerRepeatCustom,
        )
    }

    private fun dayChips(): List<CompoundButton> = with(views) {
        listOf(triggerDay1, triggerDay2, triggerDay3, triggerDay4, triggerDay5, triggerDay6, triggerDay7)
    }

    private companion object {
        const val DATE_PATTERN = "EEE d MMM yyyy"
    }
}

internal class TimeWindowFormDialog : ScenarioFormDialog<TimeWindowForm>(TimeWindowForm::class) {
    private lateinit var views: DialogScenarioTimeWindowBinding
    private lateinit var days: List<SwitchBinder>

    override val titleRes: Int = R.string.scenarios_condition_window_dialog_title

    override fun createContent(): View {
        views = DialogScenarioTimeWindowBinding.inflate(layoutInflater)
        views.windowStart.setOnClickListener { pick(TimeWindowForm::start) { form, at -> form.copy(start = at) } }
        views.windowEnd.setOnClickListener { pick(TimeWindowForm::end) { form, at -> form.copy(end = at) } }
        days = dayBinders(dayChips()) { day, checked -> edit { it.toggledDay(day, checked) } }
        return views.root
    }

    override fun render(value: TimeWindowForm) {
        views.windowStart.text = value.start.formatted()
        views.windowEnd.text = value.end.formatted()
        days.forEachIndexed { index, binder -> binder.render(index + 1 in value.weekdays) }
    }

    private fun pick(read: (TimeWindowForm) -> WallTime, write: (TimeWindowForm, WallTime) -> TimeWindowForm) {
        val at = session.value.value?.let(read) ?: return
        parentFragmentManager.showTimePicker(at.hour, at.minute) { hour, minute -> edit { write(it, WallTime(hour, minute)) } }
    }

    private fun dayChips(): List<CompoundButton> = with(views) {
        listOf(windowDay1, windowDay2, windowDay3, windowDay4, windowDay5, windowDay6, windowDay7)
    }
}

internal class LightFormDialog : ScenarioFormDialog<LightForm>(LightForm::class) {
    private lateinit var editor: LightParamsEditor

    override val titleRes: Int = R.string.scenarios_light_dialog_title

    override fun createContent(): View {
        editor = LightParamsEditor(requireContext()) { change -> edit { it.copy(editor = it.editor.apply(change)) } }
        return editor.root
    }

    override fun render(value: LightForm) = editor.render(value.editor)
}

internal fun dayBinders(chips: List<CompoundButton>, onDay: (Int, Boolean) -> Unit): List<SwitchBinder> =
    chips.mapIndexed { index, chip -> SwitchBinder(chip) { checked -> onDay(index + 1, checked) } }

internal fun WallTime.formatted(): String = String.format(Locale.ROOT, "%02d:%02d", hour, minute)
