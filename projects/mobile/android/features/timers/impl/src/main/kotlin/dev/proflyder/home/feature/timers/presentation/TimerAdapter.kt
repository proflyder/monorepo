package dev.proflyder.home.feature.timers.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.aspectPayload
import dev.proflyder.home.core.time.DurationText
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.feature.timers.impl.R
import dev.proflyder.home.feature.timers.impl.databinding.ItemTimerBinding

internal data class TimerRow(
    val timer: Timer,
    val remainingSeconds: Long,
) {
    val expired: Boolean get() = timer.state == TimerState.RUNNING && remainingSeconds == 0L
}

internal enum class TimerAspect { REMAINING, TIMER }

internal class TimerAdapterCallback(val dispatch: (TimersIntent) -> Unit) : BaseAdapterCallback<TimerRow>()

private val timerDiff = KeyedDiff<TimerRow>(
    key = { it.timer.id },
    payload = aspectPayload(mapOf(TimerAspect.REMAINING to TimerRow::remainingSeconds, TimerAspect.TIMER to TimerRow::timer)),
)

internal class TimerAdapter(
    callback: TimerAdapterCallback,
) : BaseListAdapter<TimerRow, ItemTimerBinding, TimerAdapterCallback>(callback, timerDiff) {

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
            binding.timerPauseResume.setOnClickListener { callback.dispatch(TimersIntent.TogglePause(item.timer.id)) }
            binding.timerCancel.setOnClickListener { callback.dispatch(TimersIntent.RequestCancel(item.timer.id)) }
        }

        override fun bind(item: TimerRow) {
            val timer = item.timer
            val context = binding.root.context
            binding.timerLabel.text = timer.label
            binding.timerState.text = when (timer.state) {
                TimerState.PAUSED -> context.getString(R.string.timers_paused)
                TimerState.RUNNING -> context.getString(R.string.timers_running)
                else -> timer.state.name
            }
            binding.timerPauseResume.text = context.getString(
                if (timer.state == TimerState.PAUSED) R.string.timers_resume else R.string.timers_pause,
            )
            bindRemaining(item, animate = false)
        }

        override fun bindPayloads(item: TimerRow, payloads: List<Any>) {
            val aspects = PayloadSet.merge(payloads, TimerAspect::class.java)
            when {
                aspects == null || TimerAspect.TIMER in aspects -> bind(item)
                else -> bindRemaining(item, animate = true)
            }
        }

        private fun bindRemaining(row: TimerRow, animate: Boolean) {
            binding.timerRemaining.text = DurationText.format(row.remainingSeconds)
            binding.timerProgress.setProgressCompat(progressOf(row), animate)
        }

        private fun progressOf(row: TimerRow): Int {
            val total = row.timer.durationSeconds
            if (total <= 0) return 0
            return (row.remainingSeconds.coerceIn(0, total) * PROGRESS_MAX / total).toInt()
        }
    }

    private companion object {
        const val PROGRESS_MAX = 1000
    }
}
