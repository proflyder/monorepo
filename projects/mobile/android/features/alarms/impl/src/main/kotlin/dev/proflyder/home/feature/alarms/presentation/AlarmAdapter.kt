package dev.proflyder.home.feature.alarms.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding

/** One list row: the alarm plus its multi-select presentation. */
internal data class AlarmRow(
    val alarm: Schedule,
    val selected: Boolean,
    val selectionMode: Boolean,
)

internal class AlarmAdapter(
    private val onClick: (Schedule) -> Unit,
    private val onLongClick: (Schedule) -> Unit,
    private val onToggle: (Schedule, Boolean) -> Unit,
    private val onSnooze: (Schedule) -> Unit,
    private val onStop: (Schedule) -> Unit,
    private val onDelete: (Schedule) -> Unit,
) : ListAdapter<AlarmRow, AlarmAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder =
        Holder(ItemAlarmBinding.inflate(LayoutInflater.from(parent.context), parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(private val binding: ItemAlarmBinding) : RecyclerView.ViewHolder(binding.root) {
        init {
            binding.root.isCheckable = true
        }

        fun bind(row: AlarmRow) {
            val schedule = row.alarm
            val context = binding.root.context
            binding.alarmTime.text = AlarmFormat.time(schedule)
            binding.alarmLabel.text = schedule.label
            binding.alarmRepeat.text = AlarmFormat.repeatSummary(context, schedule)
            binding.alarmNext.text = AlarmFormat.nextDue(context, schedule)

            binding.root.alpha = if (schedule.enabled) 1f else 0.6f
            binding.root.isChecked = row.selected
            binding.root.strokeWidth = if (row.selected) SELECTED_STROKE_PX else 0

            // While selecting, a tap picks the card; per-card controls are inert.
            binding.alarmActions.setVisible(!row.selectionMode)
            binding.alarmEnabled.isEnabled = !row.selectionMode
            binding.alarmEnabled.setOnCheckedChangeListener(null)
            binding.alarmEnabled.isChecked = schedule.enabled
            binding.alarmEnabled.setOnCheckedChangeListener { _, checked -> onToggle(schedule, checked) }

            binding.root.setOnClickListener { onClick(schedule) }
            binding.root.setOnLongClickListener {
                onLongClick(schedule)
                true
            }
            binding.alarmSnooze.setOnClickListener { onSnooze(schedule) }
            binding.alarmStop.setOnClickListener { onStop(schedule) }
            binding.alarmDelete.setOnClickListener { onDelete(schedule) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<AlarmRow>() {
        override fun areItemsTheSame(oldItem: AlarmRow, newItem: AlarmRow): Boolean =
            oldItem.alarm.id == newItem.alarm.id

        override fun areContentsTheSame(oldItem: AlarmRow, newItem: AlarmRow): Boolean = oldItem == newItem
    }

    private companion object {
        const val SELECTED_STROKE_PX = 4
    }
}
