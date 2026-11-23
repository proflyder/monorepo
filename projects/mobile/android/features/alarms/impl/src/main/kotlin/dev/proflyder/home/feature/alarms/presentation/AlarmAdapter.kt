package dev.proflyder.home.feature.alarms.presentation

import android.animation.ValueAnimator
import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.aspectPayload
import dev.proflyder.home.core.presentation.renderDimmed
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.designsystem.HomeMotion
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmBinding
import java.time.Instant
import kotlin.math.roundToInt

internal data class AlarmRow(
    val alarm: Schedule,
    val selected: Boolean,
    val selectionMode: Boolean,
)

internal fun shouldAnimateSelection(
    previousAlarmId: String?,
    previousSelected: Boolean?,
    alarmId: String,
    selected: Boolean,
): Boolean =
    previousAlarmId == alarmId && previousSelected != null && previousSelected != selected

internal class AlarmAdapterCallback(
    dispatch: (AlarmsIntent) -> Unit,
    val act: (AlarmActionIntent) -> Unit,
) : AdapterCallback<AlarmRow> by IntentAdapterCallback(
    dispatch = dispatch,
    onClick = { row -> AlarmsIntent.Click(row.alarm.id) },
    onLongClick = { row -> AlarmsIntent.LongClick(row.alarm.id) },
)

internal enum class AlarmAspect {
    ENABLED,
    SELECTION,
    CONTENT,
}

internal val AlarmDiff = KeyedDiff<AlarmRow>(
    key = { it.alarm.id },
    payload = aspectPayload(
        mapOf(
            AlarmAspect.ENABLED to { row: AlarmRow -> row.alarm.enabled },
            AlarmAspect.SELECTION to { row: AlarmRow -> row.selected },
            AlarmAspect.CONTENT to { row: AlarmRow -> row.copy(alarm = row.alarm.copy(enabled = false), selected = false) },
        ),
    ),
)

internal class AlarmAdapter(
    callback: AlarmAdapterCallback,
) : BaseListAdapter<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback>(callback, AlarmDiff) {

    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback> =
        Holder(ItemAlarmBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemAlarmBinding,
        callback: AlarmAdapterCallback,
    ) : BaseViewHolder<AlarmRow, ItemAlarmBinding, AlarmAdapterCallback>(binding, callback) {
        private val enabledSwitch = SwitchBinder(binding.alarmEnabled) { enabled ->
            callback.act(AlarmActionIntent.Toggle(item.alarm.id, enabled))
        }

        private var previousAlarmId: String? = null

        private var previousSelected: Boolean? = null

        private var strokeAnimator: ValueAnimator? = null

        init {
            forwardRowClicks()
            binding.alarmSnooze.setOnClickListener { callback.act(AlarmActionIntent.Snooze(item.alarm.id)) }
            binding.alarmStop.setOnClickListener { callback.act(AlarmActionIntent.Stop(item.alarm.id)) }
            binding.alarmDelete.setOnClickListener { callback.act(AlarmActionIntent.RequestDelete(item.alarm.id)) }
        }

        override fun bind(item: AlarmRow) {
            val schedule = item.alarm
            val context = binding.root.context
            bindAlarmCard(
                binding = binding,
                schedule = schedule,
                context = context,
                nowUnix = Instant.now().epochSecond,
                actionsAllowed = !item.selectionMode,
                selected = item.selected,
            )

            renderEnabled(item, animate = false)
            renderSelection(item)
        }

        override fun bindPayloads(item: AlarmRow, payloads: List<Any>) {
            val aspects = PayloadSet.merge(payloads, AlarmAspect::class.java)
            if (aspects == null || AlarmAspect.CONTENT in aspects) {
                bind(item)
                return
            }
            if (AlarmAspect.ENABLED in aspects) renderEnabled(item, animate = true)
            if (AlarmAspect.SELECTION in aspects) renderSelection(item)
        }

        private fun renderEnabled(item: AlarmRow, animate: Boolean) {
            binding.root.renderDimmed(!item.alarm.enabled && !item.selectionMode, DISABLED_ALPHA, animate)
            enabledSwitch.render(item.alarm.enabled)
        }

        private fun renderSelection(item: AlarmRow) {
            binding.root.isChecked = item.selected
            applySelection(
                selected = item.selected,
                animate = shouldAnimateSelection(previousAlarmId, previousSelected, item.alarm.id, item.selected),
            )
            previousAlarmId = item.alarm.id
            previousSelected = item.selected
        }

        private fun applySelection(selected: Boolean, animate: Boolean) {
            val context = binding.root.context
            val selectedStroke = (SELECTED_STROKE_DP * context.resources.displayMetrics.density).roundToInt()
            val targetStroke = if (selected) selectedStroke else 0
            val targetScrimAlpha = if (selected) SELECTED_SCRIM_ALPHA else 0f

            strokeAnimator?.cancel()
            strokeAnimator = null
            binding.alarmSelectionScrim.animate().cancel()

            if (!animate || !ValueAnimator.areAnimatorsEnabled()) {
                binding.root.strokeWidth = targetStroke
                binding.alarmSelectionScrim.alpha = targetScrimAlpha
                return
            }

            val startStroke = binding.root.strokeWidth
            if (startStroke != targetStroke) {
                strokeAnimator = ValueAnimator.ofFloat(startStroke.toFloat(), targetStroke.toFloat()).apply {
                    duration = HomeMotion.selectionDuration(context).toLong()
                    interpolator = HomeMotion.selectionInterpolator(context)
                    addUpdateListener { animation ->
                        binding.root.strokeWidth = (animation.animatedValue as Float).roundToInt()
                    }
                    start()
                }
            }

            binding.alarmSelectionScrim.animate()
                .alpha(targetScrimAlpha)
                .setDuration(HomeMotion.selectionDuration(context).toLong())
                .setInterpolator(HomeMotion.selectionInterpolator(context))
                .start()
        }
    }

    private companion object {
        const val SELECTED_STROKE_DP = 2f
        const val SELECTED_SCRIM_ALPHA = 0.12f
        const val DISABLED_ALPHA = 0.6f
    }
}
