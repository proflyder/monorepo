package dev.proflyder.home.feature.alarms.presentation

import android.animation.ValueAnimator
import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.designsystem.HomeMotion
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.time.Instant
import kotlin.math.roundToInt

internal data class AlarmRow(
    val alarm: Schedule,
    val selected: Boolean,
    val selectionMode: Boolean,
)

internal fun shouldAnimateSelection(
    previousAlarmId: String?,
    previousSelected: Boolean?,
    alarmId: String,
    selected: Boolean,
): Boolean =
    previousAlarmId == alarmId && previousSelected != null && previousSelected != selected

internal class AlarmAdapterCallback(
    val dispatch: (AlarmsIntent) -> Unit,
) : AdapterCallback<AlarmRow> by IntentAdapterCallback(
    dispatch = dispatch,
    onClick = { row -> AlarmsIntent.Click(row.alarm.id) },
    onLongClick = { row -> AlarmsIntent.LongClick(row.alarm.id) },
)

internal class AlarmAdapter(
    callback: AlarmAdapterCallback,
) : BaseListAdapter<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback>(callback, KeyedDiff({ it.alarm.id })) {

    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback> =
        Holder(ItemAlarmBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemAlarmBinding,
        callback: AlarmAdapterCallback,
    ) : BaseViewHolder<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback>(binding, callback) {
        private val enabledSwitch = SwitchBinder(binding.alarmEnabled) { enabled ->
            callback.dispatch(AlarmsIntent.Toggle(item.alarm.id, enabled))
        }

        private var previousAlarmId: String? = null

        private var previousSelected: Boolean? = null

        private var strokeAnimator: ValueAnimator? = null

        init {
            binding.root.isCheckable = true
            forwardRowClicks()
            binding.alarmSnooze.setOnClickListener { callback.dispatch(AlarmsIntent.Snooze(item.alarm.id)) }
            binding.alarmStop.setOnClickListener { callback.dispatch(AlarmsIntent.Stop(item.alarm.id)) }
            binding.alarmDelete.setOnClickListener { callback.dispatch(AlarmsIntent.RequestDelete(item.alarm.id)) }
        }

        override fun bind(item: AlarmRow) {
            val schedule = item.alarm
            val context = binding.root.context
            bindAlarmCard(
                binding = binding,
                schedule = schedule,
                context = context,
                nowUnix = Instant.now().epochSecond,
                actionsAllowed = !item.selectionMode,
                selected = item.selected,
            )

            binding.root.alpha = if (schedule.enabled || item.selectionMode) 1f else 0.6f
            binding.root.isChecked = item.selected
            applySelection(
                selected = item.selected,
                animate = shouldAnimateSelection(
                    previousAlarmId = previousAlarmId,
                    previousSelected = previousSelected,
                    alarmId = schedule.id,
                    selected = item.selected,
                ),
            )
            previousAlarmId = schedule.id
            previousSelected = item.selected

            enabledSwitch.render(schedule.enabled)
        }

        private fun applySelection(selected: Boolean, animate: Boolean) {
            val context = binding.root.context
            val selectedStroke = (SELECTED_STROKE_DP * context.resources.displayMetrics.density).roundToInt()
            val targetStroke = if (selected) selectedStroke else 0
            val targetScrimAlpha = if (selected) SELECTED_SCRIM_ALPHA else 0f

            strokeAnimator?.cancel()
            strokeAnimator = null
            binding.alarmSelectionScrim.animate().cancel()

            if (!animate || !ValueAnimator.areAnimatorsEnabled()) {
                binding.root.strokeWidth = targetStroke
                binding.alarmSelectionScrim.alpha = targetScrimAlpha
                return
            }

            val startStroke = binding.root.strokeWidth
            if (startStroke != targetStroke) {
                strokeAnimator = ValueAnimator.ofFloat(startStroke.toFloat(), targetStroke.toFloat()).apply {
                    duration = HomeMotion.selectionDuration(context).toLong()
                    interpolator = HomeMotion.selectionInterpolator(context)
                    addUpdateListener { animation ->
                        binding.root.strokeWidth = (animation.animatedValue as Float).roundToInt()
                    }
                    start()
                }
            }

            binding.alarmSelectionScrim.animate()
                .alpha(targetScrimAlpha)
                .setDuration(HomeMotion.selectionDuration(context).toLong())
                .setInterpolator(HomeMotion.selectionInterpolator(context))
                .start()
        }
    }

    private companion object {
        const val SELECTED_STROKE_DP = 2f
        const val SELECTED_SCRIM_ALPHA = 0.12f
    }
}
