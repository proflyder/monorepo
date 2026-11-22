package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubRowBinding

internal class HubHolder(
    private val views: ItemControlHubBinding,
    callback: ControlAdapterCallback,
) : ControlHolder(views, callback) {
    private val texts = HubCardTexts(views.root.context)

    override fun bind(item: ControlItem) {
        val hub = item as ControlItem.Hub
        views.root.setVisible(hub.cards.isNotEmpty())
        views.controlHubRows.bindChildren(hub.cards, ::createRow) { binder, card -> binder.bind(hub, card) }
    }

    private fun createRow(parent: ViewGroup): Pair<View, HubRowBinder> {
        val row = ItemControlHubRowBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return row.root to HubRowBinder(row, texts, callback.dispatch)
    }
}

internal class HubRowBinder(
    private val row: ItemControlHubRowBinding,
    private val texts: HubCardTexts,
    private val dispatch: (ControlIntent) -> Unit,
) {
    private var card: HubCard? = null
    private val alarmSwitch = SwitchBinder(row.hubRowSwitch) { enabled -> switchAlarm(enabled) }

    init {
        row.root.setOnClickListener { card?.let { dispatch(ControlIntent.Tap(TapTarget.Hub(it))) } }
        row.hubRowAction.setOnClickListener { card?.let { dispatch(ControlIntent.Hub(HubCommand.Primary(it))) } }
    }

    fun bind(hub: ControlItem.Hub, card: HubCard) {
        this.card = card
        row.hubRowIcon.setImageResource(card.icon)
        row.hubRowTime.text = texts.timeText(card, hub.remaining[card.key], hub.nowUnix)
        row.hubRowLabel.text = texts.label(card)
        row.hubRowSwitch.setVisible(card is HubCard.Alarm)
        row.hubRowAction.setVisible(card is HubCard.TimerCard)
        when (card) {
            is HubCard.Alarm -> bindAlarm(card)
            is HubCard.TimerCard -> bindTimer(card)
            is HubCard.Reminder -> Unit
        }
    }

    private fun bindAlarm(card: HubCard.Alarm) {
        row.hubRowSwitch.contentDescription = texts.title(card)
        alarmSwitch.render(card.schedule.enabled)
    }

    private fun bindTimer(card: HubCard.TimerCard) {
        val paused = card.timer.state == TimerState.PAUSED
        row.hubRowAction.setImageResource(if (paused) R.drawable.control_ic_play else R.drawable.control_ic_pause)
        row.hubRowAction.contentDescription = row.root.context.getString(
            if (paused) R.string.control_hub_resume else R.string.control_hub_pause,
        )
    }

    private fun switchAlarm(enabled: Boolean) {
        val alarm = card as? HubCard.Alarm ?: return
        dispatch(ControlIntent.Hub(HubCommand.Switch(alarm, enabled)))
    }
}

internal val HubCard.icon: Int
    get() = when (this) {
        is HubCard.Alarm -> R.drawable.control_ic_alarm
        is HubCard.TimerCard -> R.drawable.control_ic_timer
        is HubCard.Reminder -> R.drawable.control_ic_bell
    }
