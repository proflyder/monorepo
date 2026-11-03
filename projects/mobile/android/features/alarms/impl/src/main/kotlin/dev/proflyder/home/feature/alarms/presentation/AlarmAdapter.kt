package dev.proflyder.home.feature.alarms.presentation

import android.animation.ValueAnimator
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.HomeMotion
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import kotlin.math.roundToInt

/** One list row: the alarm plus its multi-select presentation. */
internal data class AlarmRow(
    val alarm: Schedule,
    val selected: Boolean,
    val selectionMode: Boolean,
)

/**
 * Selection motion is only for the same alarm changing state. A recycled
 * holder bound to a different alarm must show that alarm's selection in its
 * end state, never animate the previous row's selection into the new one.
 */
internal fun shouldAnimateSelection(
    previousAlarmId: String?,
    previousSelected: Boolean?,
    alarmId: String,
    selected: Boolean,
): Boolean =
    previousAlarmId == alarmId && previousSelected != null && previousSelected != selected

/** Row click / long click come from [AdapterCallback]; the rest are per-card actions. */
internal interface AlarmAdapterCallback : AdapterCallback<AlarmRow> {
    fun onToggle(alarm: Schedule, enabled: Boolean)
    fun onSnooze(alarm: Schedule)
    fun onStop(alarm: Schedule)
    fun onDelete(alarm: Schedule)
}

private object AlarmDiff : DiffUtil.ItemCallback<AlarmRow>() {
    override fun areItemsTheSame(oldItem: AlarmRow, newItem: AlarmRow): Boolean =
        oldItem.alarm.id == newItem.alarm.id

    override fun areContentsTheSame(oldItem: AlarmRow, newItem: AlarmRow): Boolean = oldItem == newItem
}

internal class AlarmAdapter(
    callback: AlarmAdapterCallback,
) : BaseListAdapter<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback>(callback, AlarmDiff) {

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
        /** Programmatic `isChecked` must not be reported as a user toggle. */
        private var binding_ = false

        /** Alarm shown by the previous bind; a recycled row is a new alarm. */
        private var previousAlarmId: String? = null

        /** Selection state of the previous bind, for the crossfade on change. */
        private var previousSelected: Boolean? = null

        /** Running stroke-width animator; cancel it before starting a new one. */
        private var strokeAnimator: ValueAnimator? = null

        init {
            binding.root.isCheckable = true
            forwardRowClicks()
            binding.alarmEnabled.setOnCheckedChangeListener { _, checked ->
                if (!binding_) callback.onToggle(item.alarm, checked)
            }
            binding.alarmSnooze.setOnClickListener { callback.onSnooze(item.alarm) }
            binding.alarmStop.setOnClickListener { callback.onStop(item.alarm) }
            binding.alarmDelete.setOnClickListener { callback.onDelete(item.alarm) }
        }

        override fun bind(item: AlarmRow) {
            val schedule = item.alarm
            val context = binding.root.context
            binding.alarmTime.text = AlarmFormat.time(schedule)
            binding.alarmLabel.text = schedule.label
            binding.alarmRepeat.text = AlarmFormat.repeatSummary(context, schedule)
            binding.alarmNext.text = AlarmFormat.nextDue(context, schedule)

            binding.root.alpha = if (schedule.enabled) 1f else 0.6f
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

            // While selecting, a tap picks the card; per-card controls are inert.
            binding.alarmActions.setVisible(!item.selectionMode)
            binding.alarmEnabled.isEnabled = !item.selectionMode
            binding_ = true
            binding.alarmEnabled.isChecked = schedule.enabled
            binding_ = false
        }

        /**
         * Crossfades the selection affordances: the tinted scrim fades in and
         * the selected stroke grows/shrinks over a short motion curve instead
         * of snapping with the checked state change.
         */
        private fun applySelection(selected: Boolean, animate: Boolean) {
            val context = binding.root.context
            val targetStroke = if (selected) SELECTED_STROKE_PX else 0
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
        const val SELECTED_STROKE_PX = 4
        const val SELECTED_SCRIM_ALPHA = 0.12f
    }
}
