package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubCardBinding

/** Draws the artwork of a card without a new image per entity. */
internal fun iconOf(card: HubCard): Int = when (card) {
    is HubCard.Alarm -> R.drawable.control_ic_alarm
    is HubCard.TimerCard -> R.drawable.control_ic_timer
    is HubCard.ScenarioCard -> R.drawable.control_ic_play
    is HubCard.Reminder -> R.drawable.control_ic_bell
}

/**
 * Horizontal pager of live cards. Cards are all of one kind of holder, so the
 * pager keeps its position while a timer countdown updates the same item.
 */
internal class HubCardAdapter(
    private val texts: HubCardTexts,
    private val callback: ControlAdapterCallback,
) : RecyclerView.Adapter<HubCardAdapter.Holder>() {
    private var cards: List<HubCard> = emptyList()
    private var remaining: Map<String, Long> = emptyMap()
    private var nowUnix: Long = 0

    fun submit(cards: List<HubCard>, remaining: Map<String, Long>, nowUnix: Long) {
        val oldSize = this.cards.size
        this.cards = cards
        this.remaining = remaining
        this.nowUnix = nowUnix
        if (oldSize == cards.size) {
            notifyItemRangeChanged(0, cards.size)
        } else {
            notifyItemRangeRemoved(0, oldSize)
            notifyItemRangeInserted(0, cards.size)
        }
    }

    override fun getItemCount(): Int = cards.size

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder =
        Holder(ItemControlHubCardBinding.inflate(LayoutInflater.from(parent.context), parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) {
        val card = cards[position]
        val primary = texts.primaryLabel(card)
        val secondary = texts.secondaryLabel(card)
        holder.bind(
            title = texts.title(card),
            summary = texts.summary(card, remaining[card.key], nowUnix),
            primary = primary,
            secondary = secondary,
            icon = iconOf(card),
            onClick = { callback.onHubCardClick(card) },
            onPrimary = { callback.onHubPrimary(card) },
            onSecondary = { callback.onHubSecondary(card) },
        )
    }

    internal class Holder(private val views: ItemControlHubCardBinding) : RecyclerView.ViewHolder(views.root) {
        fun bind(
            title: String,
            summary: String,
            primary: String?,
            secondary: String?,
            icon: Int,
            onClick: () -> Unit,
            onPrimary: () -> Unit,
            onSecondary: () -> Unit,
        ) {
            views.hubCardTitle.text = title
            views.hubCardSummary.text = summary
            views.hubCardIcon.setImageResource(icon)
            bindButton(views.hubCardPrimary, primary, onPrimary)
            bindButton(views.hubCardSecondary, secondary, onSecondary)
            views.root.setOnClickListener { onClick() }
        }

        private fun bindButton(button: MaterialButton, label: String?, action: () -> Unit) {
            button.setVisible(label != null)
            if (label != null) {
                button.text = label
                button.setOnClickListener { action() }
            }
        }
    }
}
