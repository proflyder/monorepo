package dev.proflyder.home.feature.alarms.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding

/** One list row: the alarm plus its multi-select presentation. */
internal data class AlarmRow(
    val alarm: Schedule,
    val selected: Boolean,
    val selectionMode: Boolean,
)

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
            binding.root.strokeWidth = if (item.selected) SELECTED_STROKE_PX else 0

            // While selecting, a tap picks the card; per-card controls are inert.
            binding.alarmActions.setVisible(!item.selectionMode)
            binding.alarmEnabled.isEnabled = !item.selectionMode
            binding_ = true
            binding.alarmEnabled.isChecked = schedule.enabled
            binding_ = false
        }
    }

    private companion object {
        const val SELECTED_STROKE_PX = 4
    }
}
