package dev.proflyder.home.feature.reminders.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.feature.reminders.impl.databinding.ItemReminderBinding

internal interface ReminderAdapterCallback : AdapterCallback<Schedule> {
    fun onToggle(reminder: Schedule, enabled: Boolean)
    fun onDelete(reminder: Schedule)
}

private object ReminderDiff : DiffUtil.ItemCallback<Schedule>() {
    override fun areItemsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem.id == newItem.id
    override fun areContentsTheSame(oldItem: Schedule, newItem: Schedule): Boolean = oldItem == newItem
}

internal class ReminderAdapter(
    callback: ReminderAdapterCallback,
) : BaseListAdapter<Schedule, ItemReminderBinding, ReminderAdapterCallback>(callback, ReminderDiff) {

    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<Schedule, ItemReminderBinding, ReminderAdapterCallback> =
        Holder(ItemReminderBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemReminderBinding,
        callback: ReminderAdapterCallback,
    ) : BaseViewHolder<Schedule, ItemReminderBinding, ReminderAdapterCallback>(binding, callback) {
        /** Programmatic `isChecked` must not be reported as a user toggle. */
        private var binding_ = false

        init {
            forwardRowClicks()
            binding.reminderEnabled.setOnCheckedChangeListener { _, checked ->
                if (!binding_) callback.onToggle(item, checked)
            }
            binding.reminderDelete.setOnClickListener { callback.onDelete(item) }
        }

        override fun bind(item: Schedule) {
            binding.reminderLabel.text = item.label
            binding.reminderDue.text = ReminderFormat.due(binding.root.context, item)
            binding.root.alpha = if (item.enabled) 1f else 0.6f
            binding_ = true
            binding.reminderEnabled.isChecked = item.enabled
            binding_ = false
        }
    }
}
