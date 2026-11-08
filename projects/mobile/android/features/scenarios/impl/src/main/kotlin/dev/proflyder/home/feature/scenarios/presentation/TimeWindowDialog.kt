package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.fragment.app.FragmentManager
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.core.ui.showTimePicker
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioTimeWindowBinding
import java.util.Locale

/** Add/edit a time window; an empty weekday set means every day. */
internal class TimeWindowDialog(
    context: Context,
    private val fragmentManager: FragmentManager,
    initial: ScenarioCondition.TimeWindow,
    private val onSave: (ScenarioCondition.TimeWindow) -> Unit,
) {
    private val views = DialogScenarioTimeWindowBinding.inflate(LayoutInflater.from(context))
    private var start = initial.start
    private var end = initial.end
    private val weekdays = initial.weekdays.filter { it in 1..7 }.toMutableSet()

    private val dayChips get() = with(views) {
        listOf(windowDay1, windowDay2, windowDay3, windowDay4, windowDay5, windowDay6, windowDay7)
    }

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_condition_window_dialog_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save) { _, _ ->
            onSave(
                ScenarioCondition.TimeWindow(
                    start = start,
                    end = end,
                    weekdays = weekdays.sorted(),
                ),
            )
        }
        .create()

    init {
        views.windowStart.setOnClickListener {
            fragmentManager.showTimePicker(start.hour, start.minute) { h, m ->
                start = WallTime(h, m)
                render()
            }
        }
        views.windowEnd.setOnClickListener {
            fragmentManager.showTimePicker(end.hour, end.minute) { h, m ->
                end = WallTime(h, m)
                render()
            }
        }
        dayChips.forEachIndexed { index, chip ->
            chip.setOnCheckedChangeListener { _, checked ->
                if (checked) weekdays.add(index + 1) else weekdays.remove(index + 1)
            }
        }
        render()
    }

    fun show() {
        dialog.setOnShowListener {
            dialog.getButton(DialogInterface.BUTTON_POSITIVE).isEnabled = true
        }
        dialog.show()
    }

    private fun render() = with(views) {
        windowStart.text = String.format(Locale.ROOT, "%02d:%02d", start.hour, start.minute)
        windowEnd.text = String.format(Locale.ROOT, "%02d:%02d", end.hour, end.minute)
        dayChips.forEachIndexed { index, chip ->
            chip.isChecked = weekdays.contains(index + 1)
        }
    }
}
