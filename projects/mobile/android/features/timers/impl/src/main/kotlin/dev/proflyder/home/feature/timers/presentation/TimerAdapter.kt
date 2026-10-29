package dev.proflyder.home.feature.timers.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.ItemTimerBinding

data class TimerRow(
    val timer: Timer,
    val remainingSeconds: Long,
)

internal interface TimerAdapterCallback : AdapterCallback<TimerRow> {
    fun onPauseResume(timer: Timer)
    fun onCancel(timer: Timer)
}

/** Only the countdown text changed; everything else on the card stays as is. */
private data class RemainingChanged(val seconds: Long)

private object TimerDiff : DiffUtil.ItemCallback<TimerRow>() {
    override fun areItemsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean =
        oldItem.timer.id == newItem.timer.id

    override fun areContentsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean = oldItem == newItem

    override fun getChangePayload(oldItem: TimerRow, newItem: TimerRow): Any? =
        if (oldItem.timer == newItem.timer) RemainingChanged(newItem.remainingSeconds) else null
}

internal class TimerAdapter(
    callback: TimerAdapterCallback,
) : BaseListAdapter<TimerRow, ItemTimerBinding, TimerAdapterCallback>(callback, TimerDiff) {

    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<TimerRow, ItemTimerBinding, TimerAdapterCallback> =
        Holder(ItemTimerBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemTimerBinding,
        callback: TimerAdapterCallback,
    ) : BaseViewHolder<TimerRow, ItemTimerBinding, TimerAdapterCallback>(binding, callback) {
        init {
            binding.timerPauseResume.setOnClickListener { callback.onPauseResume(item.timer) }
            binding.timerCancel.setOnClickListener { callback.onCancel(item.timer) }
        }

        override fun bind(item: TimerRow) {
            val timer = item.timer
            val context = binding.root.context
            binding.timerLabel.text = timer.label
            binding.timerRemaining.text = DurationText.format(item.remainingSeconds)
            binding.timerState.text = when (timer.state) {
                TimerState.PAUSED -> context.getString(R.string.timers_paused)
                TimerState.RUNNING -> context.getString(R.string.timers_running)
                else -> timer.state.name
            }
            binding.timerPauseResume.text = context.getString(
                if (timer.state == TimerState.PAUSED) R.string.timers_resume else R.string.timers_pause,
            )
        }

        override fun bindPayloads(item: TimerRow, payloads: List<Any>) {
            val remaining = payloads.filterIsInstance<RemainingChanged>().lastOrNull()
            if (remaining == null) bind(item) else binding.timerRemaining.text = DurationText.format(remaining.seconds)
        }
    }
}
