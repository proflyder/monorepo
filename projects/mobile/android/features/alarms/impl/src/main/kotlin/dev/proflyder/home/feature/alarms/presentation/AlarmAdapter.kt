package dev.proflyder.home.feature.alarms.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.alarms.impl.R

internal class AlarmAdapter(
    private val onClick: (Schedule) -> Unit,
    private val onToggle: (Schedule, Boolean) -> Unit,
    private val onSnooze: (Schedule) -> Unit,
    private val onStop: (Schedule) -> Unit,
    private val onDelete: (Schedule) -> Unit,
) : ListAdapter<Schedule, AlarmAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_alarm, parent, false)
        return Holder(view)
    }

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val time = itemView.findViewById<android.widget.TextView>(R.id.alarm_time)
        private val label = itemView.findViewById<android.widget.TextView>(R.id.alarm_label)
        private val repeat = itemView.findViewById<android.widget.TextView>(R.id.alarm_repeat)
        private val next = itemView.findViewById<android.widget.TextView>(R.id.alarm_next)
        private val enabled = itemView.findViewById<MaterialSwitch>(R.id.alarm_enabled)
        private val snooze = itemView.findViewById<android.widget.Button>(R.id.alarm_snooze)
        private val stop = itemView.findViewById<android.widget.Button>(R.id.alarm_stop)
        private val delete = itemView.findViewById<android.widget.Button>(R.id.alarm_delete)

        fun bind(schedule: Schedule) {
            val context = itemView.context
            time.text = AlarmFormat.time(schedule)
            label.text = schedule.label
            repeat.text = AlarmFormat.repeatSummary(context, schedule)
            next.text = AlarmFormat.nextDue(context, schedule)
            itemView.alpha = if (schedule.enabled) 1f else 0.6f

            enabled.setOnCheckedChangeListener(null)
            enabled.isChecked = schedule.enabled
            enabled.setOnCheckedChangeListener { _, checked -> onToggle(schedule, checked) }

            itemView.setOnClickListener { onClick(schedule) }
            snooze.setOnClickListener { onSnooze(schedule) }
            stop.setOnClickListener { onStop(schedule) }
            delete.setOnClickListener { onDelete(schedule) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<Schedule>() {
        override fun areItemsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem.id == newItem.id
        override fun areContentsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem == newItem
    }
}
