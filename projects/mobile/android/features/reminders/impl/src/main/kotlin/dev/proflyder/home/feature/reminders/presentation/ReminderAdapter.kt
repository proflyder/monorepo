package dev.proflyder.home.feature.reminders.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.reminders.impl.R

internal class ReminderAdapter(
    private val onClick: (Schedule) -> Unit,
    private val onToggle: (Schedule, Boolean) -> Unit,
    private val onDelete: (Schedule) -> Unit,
) : ListAdapter<Schedule, ReminderAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_reminder, parent, false)
        return Holder(view)
    }

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val label = itemView.findViewById<android.widget.TextView>(R.id.reminder_label)
        private val due = itemView.findViewById<android.widget.TextView>(R.id.reminder_due)
        private val enabled = itemView.findViewById<MaterialSwitch>(R.id.reminder_enabled)
        private val delete = itemView.findViewById<android.widget.Button>(R.id.reminder_delete)

        fun bind(schedule: Schedule) {
            label.text = schedule.label
            due.text = ReminderFormat.due(itemView.context, schedule)
            itemView.alpha = if (schedule.enabled) 1f else 0.6f

            enabled.setOnCheckedChangeListener(null)
            enabled.isChecked = schedule.enabled
            enabled.setOnCheckedChangeListener { _, checked -> onToggle(schedule, checked) }
            itemView.setOnClickListener { onClick(schedule) }
            delete.setOnClickListener { onDelete(schedule) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<Schedule>() {
        override fun areItemsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem.id == newItem.id
        override fun areContentsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem == newItem
    }
}
