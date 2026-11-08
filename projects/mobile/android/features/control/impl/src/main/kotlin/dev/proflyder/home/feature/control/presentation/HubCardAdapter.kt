package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubAlarmBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubCardBinding

/** Draws the artwork of a card without a new image per entity. */
internal fun iconOf(card: HubCard): Int = when (card) {
    is HubCard.Alarm -> R.drawable.control_ic_alarm
    is HubCard.TimerCard -> R.drawable.control_ic_timer
    is HubCard.ScenarioCard -> R.drawable.control_ic_play
    is HubCard.Reminder -> R.drawable.control_ic_bell
}

/**
 * One pager item. [nowUnix] is part of the identity of the rendered text, so a
 * refresh that only re-reads the same data does not touch the pager at all.
 */
internal data class HubCardItem(
    val card: HubCard,
    val remaining: Long?,
    val nowUnix: Long,
) {
    val key: String get() = card.key
}

private object HubCardDiff : DiffUtil.ItemCallback<HubCardItem>() {
    override fun areItemsTheSame(oldItem: HubCardItem, newItem: HubCardItem): Boolean =
        oldItem.key == newItem.key

    override fun areContentsTheSame(oldItem: HubCardItem, newItem: HubCardItem): Boolean =
        oldItem == newItem
}

/**
 * Horizontal pager of live cards. Stable ids and a granular diff keep the
 * current card, its buttons and the scroll position intact while the Home
 * screen refreshes; only a card whose content actually changed is rebound.
 */
internal class HubCardAdapter(
    private val texts: HubCardTexts,
    private val callback: ControlAdapterCallback,
) : ListAdapter<HubCardItem, RecyclerView.ViewHolder>(HubCardDiff) {
    init {
        setHasStableIds(true)
    }

    fun submit(cards: List<HubCard>, remaining: Map<String, Long>, nowUnix: Long) {
        submitList(cards.map { HubCardItem(it, remaining[it.key], nowUnix) })
    }

    override fun getItemId(position: Int): Long = getItem(position).key.hashCode().toLong()

    override fun getItemViewType(position: Int): Int =
        if (getItem(position).card is HubCard.Alarm) TYPE_ALARM else TYPE_CARD

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            TYPE_ALARM -> AlarmHolder(ItemControlHubAlarmBinding.inflate(inflater, parent, false))
            else -> CardHolder(ItemControlHubCardBinding.inflate(inflater, parent, false))
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        val item = getItem(position)
        when (holder) {
            is AlarmHolder -> holder.bind(item)
            is CardHolder -> holder.bind(item)
            else -> Unit
        }
    }

    /** Compact alarm card: the time is the headline, label and days share a line. */
    internal inner class AlarmHolder(
        private val views: ItemControlHubAlarmBinding,
    ) : RecyclerView.ViewHolder(views.root) {
        fun bind(item: HubCardItem) {
            val card = item.card as HubCard.Alarm
            views.hubAlarmTime.text = texts.alarmTime(card)
            views.hubAlarmSubtitle.text = texts.alarmSubtitle(card)
            views.hubAlarmIcon.setImageResource(iconOf(card))
            bindButton(views.hubAlarmPrimary, texts.primaryLabel(card)) { callback.onHubPrimary(card) }
            bindButton(views.hubAlarmSecondary, texts.secondaryLabel(card)) { callback.onHubSecondary(card) }
            views.root.setOnClickListener { callback.onHubCardClick(card) }
        }
    }

    /** Timer, scenario and reminder cards keep the roomy shared card. */
    internal inner class CardHolder(
        private val views: ItemControlHubCardBinding,
    ) : RecyclerView.ViewHolder(views.root) {
        fun bind(item: HubCardItem) {
            val card = item.card
            views.hubCardTitle.text = texts.title(card)
            views.hubCardSummary.text = texts.summary(card, item.remaining, item.nowUnix)
            views.hubCardIcon.setImageResource(iconOf(card))
            bindButton(views.hubCardPrimary, texts.primaryLabel(card)) { callback.onHubPrimary(card) }
            bindButton(views.hubCardSecondary, texts.secondaryLabel(card)) { callback.onHubSecondary(card) }
            views.root.setOnClickListener { callback.onHubCardClick(card) }
        }
    }

    private fun bindButton(button: MaterialButton, label: String?, action: () -> Unit) {
        button.setVisible(label != null)
        if (label != null) {
            button.text = label
            button.setOnClickListener { action() }
        }
    }

    private companion object {
        const val TYPE_CARD = 0
        const val TYPE_ALARM = 1
    }
}
