package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlSpeedDialBinding

/** One "icon + label" row of the Home quick-actions speed dial. */
internal class SpeedDialAdapter(
    private val onClick: (SpeedDialItem) -> Unit,
    private val onLongClick: (SpeedDialItem) -> Unit,
) : ListAdapter<SpeedDialItem, SpeedDialAdapter.Holder>(Diff) {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder =
        Holder(ItemControlSpeedDialBinding.inflate(LayoutInflater.from(parent.context), parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) = holder.bind(getItem(position))

    internal inner class Holder(
        private val views: ItemControlSpeedDialBinding,
    ) : RecyclerView.ViewHolder(views.root) {
        internal fun bind(item: SpeedDialItem) {
            val context = views.root.context
            val label = item.label(context)
            val state = item.state(context)
            val enabled = item.isEnabled()
            views.controlDialTitle.text = label
            views.controlDialState.text = state.orEmpty()
            views.controlDialState.setVisible(state != null)
            views.controlDialIcon.setImageResource(item.icon())
            views.controlDialIcon.isActivated = item.isActive()
            views.controlDialIcon.setColorFilter(
                MaterialColors.getColor(
                    views.root,
                    if (item.isActive()) com.google.android.material.R.attr.colorOnPrimaryContainer
                    else com.google.android.material.R.attr.colorOnSurfaceVariant,
                ),
            )
            views.root.alpha = if (enabled) 1f else DISABLED_ALPHA
            views.root.isEnabled = enabled
            views.root.contentDescription = when (state) {
                null -> label
                else -> context.getString(R.string.control_tile_description, label, state)
            }
            views.root.setOnClickListener { if (enabled) onClick(item) }
            views.root.setOnLongClickListener {
                onLongClick(item)
                true
            }
        }
    }

    private object Diff : DiffUtil.ItemCallback<SpeedDialItem>() {
        override fun areItemsTheSame(oldItem: SpeedDialItem, newItem: SpeedDialItem): Boolean =
            oldItem.key == newItem.key

        override fun areContentsTheSame(oldItem: SpeedDialItem, newItem: SpeedDialItem): Boolean =
            oldItem == newItem
    }

    private companion object {
        const val DISABLED_ALPHA = 0.5f
    }
}

internal fun SpeedDialItem.label(context: Context): String = when (this) {
    is SpeedDialItem.Tile -> tile.label(context)
    is SpeedDialItem.Wake -> context.getString(
        when (action.kind) {
            WakeFeedbackKind.FALSE_ALARM -> R.string.control_wake_false_alarm
            WakeFeedbackKind.MISSED -> R.string.control_wake_missed
        },
    )
}

private fun QuickTile.label(context: Context): String = when (this) {
    is QuickTile.Device -> entry.localizedDisplayName(context)
    is QuickTile.ScenarioTile -> scenario.label
    is QuickTile.Speak -> entry.localizedDisplayName(context)
    QuickTile.Call -> context.getString(R.string.control_call_title)
}

internal fun SpeedDialItem.state(context: Context): String? = when (this) {
    is SpeedDialItem.Tile -> (tile as? QuickTile.Device)
        ?.entry
        ?.localizedStateText(context, R.string.control_state_unavailable)
    is SpeedDialItem.Wake -> null
}

internal fun SpeedDialItem.icon(): Int = when (this) {
    is SpeedDialItem.Tile -> tile.icon()
    is SpeedDialItem.Wake -> when (action.kind) {
        WakeFeedbackKind.FALSE_ALARM -> R.drawable.control_ic_ear_off
        WakeFeedbackKind.MISSED -> R.drawable.control_ic_ear
    }
}

private fun QuickTile.icon(): Int = when (this) {
    is QuickTile.Device -> ControlIcons.resolve(entry)
    is QuickTile.ScenarioTile -> R.drawable.control_ic_play_circle
    is QuickTile.Speak -> R.drawable.control_ic_voice
    QuickTile.Call -> R.drawable.control_ic_call
}

internal fun SpeedDialItem.isActive(): Boolean = when (this) {
    is SpeedDialItem.Tile -> (tile as? QuickTile.Device)?.entry?.power == PowerState.ON || tile !is QuickTile.Device
    is SpeedDialItem.Wake -> true
}

internal fun SpeedDialItem.isEnabled(): Boolean = when (this) {
    is SpeedDialItem.Tile -> when (val current = tile) {
        is QuickTile.Device -> current.entry.available && !current.busy
        is QuickTile.ScenarioTile -> !current.busy
        is QuickTile.Speak -> current.entry.available && !current.busy
        QuickTile.Call -> true
    }
    is SpeedDialItem.Wake -> action.entry.available && !action.busy
}
