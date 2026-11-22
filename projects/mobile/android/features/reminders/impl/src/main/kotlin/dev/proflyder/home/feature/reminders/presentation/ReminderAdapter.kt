package dev.proflyder.home.feature.reminders.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.aspectPayload
import dev.proflyder.home.core.presentation.renderDimmed
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.feature.reminders.impl.databinding.ItemReminderBinding

internal class ReminderAdapterCallback(
    val dispatch: (RemindersIntent) -> Unit,
) : AdapterCallback<Schedule> by IntentAdapterCallback(dispatch, { RemindersIntent.Open(it.id) })

internal enum class ReminderAspect {
    ENABLED,
    CONTENT,
}

internal val ReminderDiff = KeyedDiff<Schedule>(
    key = Schedule::id,
    payload = aspectPayload(
        mapOf(
            ReminderAspect.ENABLED to { item: Schedule -> item.enabled },
            ReminderAspect.CONTENT to { item: Schedule -> item.copy(enabled = false) },
        ),
    ),
)

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
            renderEnabled(item, animate = false)
        }

        override fun bindPayloads(item: Schedule, payloads: List<Any>) {
            val aspects = PayloadSet.merge(payloads, ReminderAspect::class.java)
            if (aspects == null || ReminderAspect.CONTENT in aspects) bind(item) else renderEnabled(item, animate = true)
        }

        private fun renderEnabled(item: Schedule, animate: Boolean) {
            binding.root.renderDimmed(!item.enabled, DISABLED_ALPHA, animate)
            enabledSwitch.render(item.enabled)
        }
    }

    private companion object {
        const val DISABLED_ALPHA = 0.6f
    }
}
