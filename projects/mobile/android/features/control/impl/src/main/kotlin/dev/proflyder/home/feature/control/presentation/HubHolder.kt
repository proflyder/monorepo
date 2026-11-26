package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.designsystem.R as DesignR
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import dev.proflyder.home.core.model.TimerState
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

    init {
        row.root.setOnClickListener { card?.let { dispatch(ControlIntent.Tap(TapTarget.Hub(it))) } }
        row.hubRowAction.setOnClickListener { card?.let { dispatch(ControlIntent.Hub(HubCommand.Primary(it))) } }
    }

    fun bind(hub: ControlItem.Hub, card: HubCard) {
        this.card = card
        row.hubRowIcon.setImageResource(card.icon)
        row.hubRowTime.text = texts.timeText(card, hub.remaining[card.key], hub.nowUnix)
        row.hubRowLabel.text = texts.label(card)
        row.hubRowAction.setVisible(card is HubCard.TimerCard)
        row.hubRowIcon.isActivated = card.isLive
        when (card) {
            is HubCard.TimerCard -> bindTimer(card)
            is HubCard.Reminder -> Unit
        }
    }

    private fun bindTimer(card: HubCard.TimerCard) {
        val paused = card.timer.state == TimerState.PAUSED
        row.hubRowAction.setImageResource(if (paused) R.drawable.control_ic_play else R.drawable.control_ic_pause)
        row.hubRowAction.contentDescription = row.root.context.getString(
            if (paused) DesignR.string.ds_action_resume else DesignR.string.ds_action_pause,
        )
    }
}

internal val HubCard.isLive: Boolean
    get() = when (this) {
        is HubCard.TimerCard -> timer.state == TimerState.RUNNING
        is HubCard.Reminder -> schedule.enabled
    }

internal val HubCard.icon: Int
    get() = when (this) {
        is HubCard.TimerCard -> R.drawable.control_ic_timer
        is HubCard.Reminder -> R.drawable.control_ic_bell
    }
