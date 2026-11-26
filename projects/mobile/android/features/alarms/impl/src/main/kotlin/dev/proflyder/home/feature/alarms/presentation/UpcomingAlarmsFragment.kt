package dev.proflyder.home.feature.alarms.presentation

import android.content.Context
import android.os.Bundle
import androidx.annotation.AttrRes
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.presentation.SectionFragment
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.time.DueText
import dev.proflyder.home.core.time.DueTextStrings
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.alarms.impl.R
import dev.proflyder.home.feature.alarms.impl.databinding.FragmentAlarmsUpcomingBinding
import dev.proflyder.home.feature.alarms.impl.databinding.ItemAlarmsUpcomingBinding
import dev.proflyder.home.core.presentation.renderSlice
import java.time.DayOfWeek
import java.time.Instant
import java.time.ZoneId
import java.time.format.TextStyle
import java.util.Locale

internal class UpcomingAlarmsFragment :
    SectionFragment<FragmentAlarmsUpcomingBinding, UpcomingAlarmsState, UpcomingAlarmsIntent, UpcomingAlarmsEvent>(
        FragmentAlarmsUpcomingBinding.Companion::inflate,
    ) {
    override val viewModel: UpcomingAlarmsViewModel by screenViewModel()

    private val prompts = AlarmPrompts(this) { dispatch(UpcomingAlarmsIntent.Act(it)) }

    override fun onViewReady(binding: FragmentAlarmsUpcomingBinding, savedInstanceState: Bundle?) {
        val texts = UpcomingAlarmTexts(requireContext())
        val rows = listOf(binding.alarmsUpcomingFirst, binding.alarmsUpcomingSecond, binding.alarmsUpcomingThird)
            .map { UpcomingAlarmRow(ItemAlarmsUpcomingBinding.bind(it), texts, ::dispatch) }
        render({ it }) { state -> renderRows(rows, state, viewModel.minute.value) }
        viewLifecycleOwner.renderSlice(viewModel.minute, { it }) { minute -> renderRows(rows, viewModel.state.value, minute) }
    }

    private fun renderRows(rows: List<UpcomingAlarmRow>, state: UpcomingAlarmsState, minute: Long) {
        binding.root.setVisible(state.alarms.isNotEmpty())
        val now = Instant.ofEpochSecond(minute * SECONDS_PER_MINUTE)
        rows.forEachIndexed { index, row -> row.bind(state.alarms.getOrNull(index), state.nowUnix, now) }
    }

    override fun onEvent(event: UpcomingAlarmsEvent) {
        when (event) {
            is UpcomingAlarmsEvent.Action -> prompts.show(event.event)
            is UpcomingAlarmsEvent.OpenRoute -> navigate(event.route)
        }
    }
}

internal class UpcomingAlarmRow(
    private val views: ItemAlarmsUpcomingBinding,
    private val texts: UpcomingAlarmTexts,
    private val dispatch: (UpcomingAlarmsIntent) -> Unit,
) {
    private var id: String? = null
    private val enabled = SwitchBinder(views.alarmsUpcomingSwitch) { checked -> toggle(checked) }

    init {
        views.root.setOnClickListener { id?.let { dispatch(UpcomingAlarmsIntent.Open(it)) } }
    }

    fun bind(item: UpcomingAlarm?, nowUnix: Long, now: Instant) {
        views.root.setVisible(item != null)
        id = item?.alarm?.id
        item ?: return
        val detail = texts.detail(item, now)
        views.alarmsUpcomingTitle.text = texts.headline(item, nowUnix)
        views.alarmsUpcomingDetail.text = detail.text
        views.alarmsUpcomingDetail.setTextColor(MaterialColors.getColor(views.alarmsUpcomingDetail, detail.colorAttr))
        views.alarmsUpcomingIcon.isActivated = item.alarm.enabled
        views.alarmsUpcomingSwitch.contentDescription = texts.title(item)
        enabled.render(item.alarm.enabled)
    }

    private fun toggle(checked: Boolean) {
        id?.let { dispatch(UpcomingAlarmsIntent.Act(AlarmActionIntent.Toggle(it, checked))) }
    }
}

internal class UpcomingAlarmTexts(private val context: Context) : DueTextStrings {
    override fun today(): String = context.getString(R.string.alarms_upcoming_today)

    override fun tomorrow(): String = context.getString(R.string.alarms_upcoming_tomorrow)

    override fun dayOfWeek(isoDay: Int): String = DayOfWeek.of(isoDay).getDisplayName(TextStyle.SHORT, Locale.getDefault())

    fun title(item: UpcomingAlarm): String = item.alarm.label.ifBlank { context.getString(R.string.alarms_label_default) }

    fun headline(item: UpcomingAlarm, nowUnix: Long): String =
        context.getString(R.string.alarms_upcoming_subtitle, time(item, nowUnix), title(item))

    fun detail(item: UpcomingAlarm, now: Instant): UpcomingDetail = when {
        item.ringing -> UpcomingDetail.accent(context.getString(R.string.alarms_upcoming_ringing))
        else -> countdown(item, now)?.let(UpcomingDetail::accent)
            ?: UpcomingDetail.quiet(AlarmFormat.repeatSummary(context, item.alarm))
    }

    private fun countdown(item: UpcomingAlarm, now: Instant): String? =
        AlarmCountdownText.card(context.resources, item.alarm, now, ZoneId.systemDefault())

    private fun time(item: UpcomingAlarm, nowUnix: Long): String {
        val due = if (item.ringing) item.alarm.activeOccurrenceUnix else item.alarm.nextDueUnix
        return DueText.format(due, ZoneId.systemDefault(), nowUnix, this)
    }
}

internal data class UpcomingDetail(val text: String, @AttrRes val colorAttr: Int) {
    companion object {
        fun accent(text: String) = UpcomingDetail(text, androidx.appcompat.R.attr.colorPrimary)

        fun quiet(text: String) = UpcomingDetail(text, com.google.android.material.R.attr.colorOnSurfaceVariant)
    }
}
