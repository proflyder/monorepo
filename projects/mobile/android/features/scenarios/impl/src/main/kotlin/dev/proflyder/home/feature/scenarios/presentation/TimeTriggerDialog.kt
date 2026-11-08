package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.fragment.app.FragmentManager
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.showDatePicker
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioTimeTriggerBinding
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Add/edit one time trigger; the timezone is the phone's system IANA zone. */
internal class TimeTriggerDialog(
    context: Context,
    private val fragmentManager: FragmentManager,
    initial: TimeTrigger?,
    private val onSave: (TimeTrigger) -> Unit,
) {
    private val views = DialogScenarioTimeTriggerBinding.inflate(LayoutInflater.from(context))
    private val zone: ZoneId = runCatching { ZoneId.of(initial?.timezone.orEmpty()) }.getOrDefault(ZoneId.systemDefault())
    private var hour = initial?.at?.hour ?: 7
    private var minute = initial?.at?.minute ?: 30
    private var repeat = initial?.repeat?.takeIf { it != RepeatKind.UNSPECIFIED } ?: RepeatKind.DAILY
    private var onceDate: LocalDate = runCatching { LocalDate.parse(initial?.onceDate.orEmpty()) }
        .getOrNull() ?: LocalDate.now(zone)
    private val weekdays = (initial?.weekdays.orEmpty().filter { it in 1..7 }).toMutableSet()

    private val dayChips get() = with(views) {
        listOf(triggerDay1, triggerDay2, triggerDay3, triggerDay4, triggerDay5, triggerDay6, triggerDay7)
    }

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_trigger_dialog_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save, null)
        .create()

    init {
        views.triggerTimezone.text = zone.id
        views.triggerTime.setOnClickListener {
            fragmentManager.showTimePicker(hour, minute) { h, m ->
                hour = h
                minute = m
                render()
            }
        }
        views.triggerRepeatGroup.setOnCheckedStateChangeListener { _, checked ->
            if (checked.isEmpty()) return@setOnCheckedStateChangeListener
            repeat = when (checked.first()) {
                R.id.trigger_repeat_once -> RepeatKind.ONCE
                R.id.trigger_repeat_weekdays -> RepeatKind.WEEKDAYS
                R.id.trigger_repeat_weekends -> RepeatKind.WEEKENDS
                R.id.trigger_repeat_custom -> RepeatKind.CUSTOM
                else -> RepeatKind.DAILY
            }
            render()
        }
        views.triggerDate.setOnClickListener {
            fragmentManager.showDatePicker(onceDate, LocalDate.now(zone)) { date ->
                onceDate = date
                render()
            }
        }
        dayChips.forEachIndexed { index, chip ->
            chip.setOnCheckedChangeListener { _, checked ->
                if (checked) weekdays.add(index + 1) else weekdays.remove(index + 1)
                renderPositive()
            }
        }
        render()
    }

    fun show() {
        dialog.setOnShowListener { renderPositive() }
        dialog.show()
    }

    private fun render() = with(views) {
        triggerTime.text = String.format(Locale.ROOT, "%02d:%02d", hour, minute)
        triggerRepeatGroup.check(
            when (repeat) {
                RepeatKind.ONCE -> R.id.trigger_repeat_once
                RepeatKind.WEEKDAYS -> R.id.trigger_repeat_weekdays
                RepeatKind.WEEKENDS -> R.id.trigger_repeat_weekends
                RepeatKind.CUSTOM -> R.id.trigger_repeat_custom
                else -> R.id.trigger_repeat_daily
            },
        )
        triggerDate.setVisible(repeat == RepeatKind.ONCE)
        triggerDate.text = onceDate.format(DateTimeFormatter.ofPattern("EEE d MMM yyyy", Locale.getDefault()))
        triggerDaysLabel.setVisible(repeat == RepeatKind.CUSTOM)
        triggerDaysGroup.setVisible(repeat == RepeatKind.CUSTOM)
        dayChips.forEachIndexed { index, chip ->
            if (chip.isChecked != weekdays.contains(index + 1)) chip.isChecked = weekdays.contains(index + 1)
        }
        renderPositive()
    }

    private fun renderPositive() {
        val button = runCatching { dialog.getButton(DialogInterface.BUTTON_POSITIVE) }.getOrNull() ?: return
        button.isEnabled = when (repeat) {
            RepeatKind.CUSTOM -> weekdays.isNotEmpty()
            RepeatKind.ONCE -> !onceDate.isBefore(LocalDate.now(zone))
            else -> true
        }
        button.setOnClickListener {
            if (!button.isEnabled) return@setOnClickListener
            onSave(
                TimeTrigger(
                    timezone = zone.id,
                    at = WallTime(hour, minute),
                    repeat = repeat,
                    onceDate = if (repeat == RepeatKind.ONCE) onceDate.toString() else "",
                    weekdays = if (repeat == RepeatKind.CUSTOM) weekdays.sorted() else emptyList(),
                ),
            )
            dialog.dismiss()
        }
    }
}
