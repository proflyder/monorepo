package dev.proflyder.home.feature.timers.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.feature.timers.impl.R

data class TimerRow(
    val timer: Timer,
    val remainingSeconds: Long,
)

internal class TimerAdapter(
    private val onPauseResume: (Timer) -> Unit,
    private val onCancel: (Timer) -> Unit,
) : ListAdapter<TimerRow, TimerAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_timer, parent, false)
        return Holder(view)
    }

    override fun onBindViewHolder(holder: Holder, position: Int) {
        holder.bind(getItem(position))
    }

    inner class Holder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val label = itemView.findViewById<android.widget.TextView>(R.id.timer_label)
        private val state = itemView.findViewById<android.widget.TextView>(R.id.timer_state)
        private val remaining = itemView.findViewById<android.widget.TextView>(R.id.timer_remaining)
        private val pauseResume = itemView.findViewById<android.widget.Button>(R.id.timer_pause_resume)
        private val cancel = itemView.findViewById<android.widget.Button>(R.id.timer_cancel)

        fun bind(row: TimerRow) {
            val timer = row.timer
            label.text = timer.label
            remaining.text = DurationText.format(row.remainingSeconds)
            state.text = when (timer.state) {
                TimerState.PAUSED -> itemView.context.getString(R.string.timers_paused)
                TimerState.RUNNING -> itemView.context.getString(R.string.timers_running)
                else -> timer.state.name
            }
            pauseResume.text = if (timer.state == TimerState.PAUSED) {
                itemView.context.getString(R.string.timers_resume)
            } else {
                itemView.context.getString(R.string.timers_pause)
            }
            pauseResume.setOnClickListener { onPauseResume(timer) }
            cancel.setOnClickListener { onCancel(timer) }
        }
    }

    private object Diff : DiffUtil.ItemCallback<TimerRow>() {
        override fun areItemsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean =
            oldItem.timer.id == newItem.timer.id

        override fun areContentsTheSame(oldItem: TimerRow, newItem: TimerRow): Boolean = oldItem == newItem
    }
}
