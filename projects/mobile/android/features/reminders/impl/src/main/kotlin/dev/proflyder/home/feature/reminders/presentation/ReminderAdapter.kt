package dev.proflyder.home.feature.reminders.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.feature.reminders.impl.databinding.ItemReminderBinding

internal class ReminderAdapter(
    private val onClick: (Schedule) -> Unit,
    private val onToggle: (Schedule, Boolean) -> Unit,
    private val onDelete: (Schedule) -> Unit,
) : ListAdapter<Schedule, ReminderAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder =
        Holder(ItemReminderBinding.inflate(LayoutInflater.from(parent.context), parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(private val binding: ItemReminderBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(schedule: Schedule) {
            binding.reminderLabel.text = schedule.label
            binding.reminderDue.text = ReminderFormat.due(binding.root.context, schedule)
            binding.root.alpha = if (schedule.enabled) 1f else 0.6f

            binding.reminderEnabled.setOnCheckedChangeListener(null)
            binding.reminderEnabled.isChecked = schedule.enabled
            binding.reminderEnabled.setOnCheckedChangeListener { _, checked -> onToggle(schedule, checked) }
            binding.root.setOnClickListener { onClick(schedule) }
            binding.reminderDelete.setOnClickListener { onDelete(schedule) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<Schedule>() {
        override fun areItemsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem.id == newItem.id
        override fun areContentsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem == newItem
    }
}
