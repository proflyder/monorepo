package dev.proflyder.home.feature.reminders.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.feature.reminders.impl.databinding.ItemReminderBinding

internal class ReminderAdapterCallback(
    val dispatch: (RemindersIntent) -> Unit,
) : AdapterCallback<Schedule> by IntentAdapterCallback(dispatch, { RemindersIntent.Open(it.id) })

internal class ReminderAdapter(
    callback: ReminderAdapterCallback,
) : BaseListAdapter<Schedule, ItemReminderBinding, ReminderAdapterCallback>(callback, KeyedDiff(Schedule::id)) {

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
        private val enabledSwitch = SwitchBinder(binding.reminderEnabled) { enabled ->
            callback.dispatch(RemindersIntent.Toggle(item.id, enabled))
        }

        init {
            forwardRowClicks()
            binding.reminderDelete.setOnClickListener { callback.dispatch(RemindersIntent.RequestDelete(item.id)) }
        }

        override fun bind(item: Schedule) {
            binding.reminderLabel.text = item.label
            binding.reminderDue.text = ReminderFormat.due(binding.root.context, item)
            binding.root.alpha = if (item.enabled) 1f else DISABLED_ALPHA
            enabledSwitch.render(item.enabled)
        }
    }

    private companion object {
        const val DISABLED_ALPHA = 0.6f
    }
}
