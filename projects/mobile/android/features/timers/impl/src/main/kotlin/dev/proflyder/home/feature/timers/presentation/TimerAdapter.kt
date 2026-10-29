package dev.proflyder.home.feature.timers.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.ItemTimerBinding

data class TimerRow(
    val timer: Timer,
    val remainingSeconds: Long,
)

internal class TimerAdapter(
    private val onPauseResume: (Timer) -> Unit,
    private val onCancel: (Timer) -> Unit,
) : ListAdapter<TimerRow, TimerAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder =
        Holder(ItemTimerBinding.inflate(LayoutInflater.from(parent.context), parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(private val binding: ItemTimerBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(row: TimerRow) {
            val timer = row.timer
            val context = binding.root.context
            binding.timerLabel.text = timer.label
            binding.timerRemaining.text = DurationText.format(row.remainingSeconds)
            binding.timerState.text = when (timer.state) {
                TimerState.PAUSED -> context.getString(R.string.timers_paused)
                TimerState.RUNNING -> context.getString(R.string.timers_running)
                else -> timer.state.name
            }
            binding.timerPauseResume.text = context.getString(
                if (timer.state == TimerState.PAUSED) R.string.timers_resume else R.string.timers_pause,
            )
            binding.timerPauseResume.setOnClickListener { onPauseResume(timer) }
            binding.timerCancel.setOnClickListener { onCancel(timer) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<TimerRow>() {
        override fun areItemsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean =
            oldItem.timer.id == newItem.timer.id

        override fun areContentsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean = oldItem == newItem
    }
}
