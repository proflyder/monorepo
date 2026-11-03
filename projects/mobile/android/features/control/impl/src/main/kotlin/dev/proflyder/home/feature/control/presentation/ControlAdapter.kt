package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHeaderBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlProfileBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

/** Row taps come from [AdapterCallback]; the switch reports an explicit target state. */
internal interface ControlAdapterCallback : AdapterCallback<ControlItem> {
    fun onPowerChanged(entry: ControlEntry, on: Boolean)

    /** The profile selector reports the chosen side: `work = true` is the named profile. */
    fun onProfileChanged(entry: ControlEntry, work: Boolean)
}

/**
 * Aspects of a Home row that [ControlDiff] can report as changed. A holder
 * updates only the views these map to, so an optimistic update or a server
 * re-read does not rebuild (and crossfade) the whole card.
 */
internal enum class ControlChange {
    /** Row title text. */
    TITLE,

    /** Artwork selected from the entry kind/icon hint. */
    ICON,

    /** On/off state: switch or profile side, icon tint/activation, state text. */
    POWER,

    /** Command-in-flight indicator and the enabled state that depends on it. */
    BUSY,

    /** Availability: dimming, enabled state and state text. */
    AVAILABILITY,

    /** State text (power, availability, or a light's brightness reading). */
    STATE,

    /** A section header's right-aligned summary. */
    SUMMARY,
}

/** Identity of the artwork an entry resolves to; see [ControlIcons]. */
private fun ControlEntry.iconIdentity(): Pair<Any, String> = kind to icon.lowercase()

/** Typed payloads delivered for one bind, merged into a single aspect set. */
private fun controlChanges(payloads: List<Any>): PayloadSet<ControlChange>? =
    PayloadSet.merge(payloads, ControlChange::class.java)

internal object ControlDiff : DiffUtil.ItemCallback<ControlItem>() {
    override fun areItemsTheSame(oldItem: ControlItem, newItem: ControlItem): Boolean = oldItem.key == newItem.key

    override fun areContentsTheSame(oldItem: ControlItem, newItem: ControlItem): Boolean = oldItem == newItem

    /**
     * A typed set of changed aspects, or null when the difference has no view
     * representation (or the row type changed), in which case the holder does a
     * full bind.
     */
    override fun getChangePayload(oldItem: ControlItem, newItem: ControlItem): Any? {
        val changed = when {
            oldItem is ControlItem.Header && newItem is ControlItem.Header -> headerChanges(oldItem, newItem)
            oldItem is ControlItem.Profile && newItem is ControlItem.Profile -> profileChanges(oldItem, newItem)
            oldItem is ControlItem.Scenario && newItem is ControlItem.Scenario -> scenarioChanges(oldItem, newItem)
            oldItem is ControlItem.Device && newItem is ControlItem.Device -> deviceChanges(oldItem, newItem)
            else -> emptySet()
        }
        return if (changed.isEmpty()) null else PayloadSet.of(changed)
    }

    private fun headerChanges(old: ControlItem.Header, new: ControlItem.Header): Set<ControlChange> = buildSet {
        if (old.title != new.title) add(ControlChange.TITLE)
        if (old.summary != new.summary) add(ControlChange.SUMMARY)
    }

    private fun profileChanges(old: ControlItem.Profile, new: ControlItem.Profile): Set<ControlChange> = buildSet {
        val before = old.entry
        val after = new.entry
        if (before.title != after.title) add(ControlChange.TITLE)
        if (before.iconIdentity() != after.iconIdentity()) add(ControlChange.ICON)
        if (before.power != after.power) {
            add(ControlChange.POWER)
            add(ControlChange.STATE)
        }
        if (before.available != after.available) {
            add(ControlChange.AVAILABILITY)
            add(ControlChange.STATE)
        }
        if (old.busy != new.busy) add(ControlChange.BUSY)
    }

    private fun scenarioChanges(old: ControlItem.Scenario, new: ControlItem.Scenario): Set<ControlChange> = buildSet {
        val before = old.entry
        val after = new.entry
        if (before.title != after.title) add(ControlChange.TITLE)
        if (before.iconIdentity() != after.iconIdentity()) add(ControlChange.ICON)
        if (before.available != after.available) add(ControlChange.AVAILABILITY)
        if (old.busy != new.busy) add(ControlChange.BUSY)
    }

    private fun deviceChanges(old: ControlItem.Device, new: ControlItem.Device): Set<ControlChange> = buildSet {
        val before = old.entry
        val after = new.entry
        if (before.title != after.title) add(ControlChange.TITLE)
        if (before.iconIdentity() != after.iconIdentity()) add(ControlChange.ICON)
        if (before.power != after.power) {
            add(ControlChange.POWER)
            add(ControlChange.STATE)
        }
        if (before.available != after.available) {
            add(ControlChange.AVAILABILITY)
            add(ControlChange.STATE)
        }
        if (old.busy != new.busy) add(ControlChange.BUSY)
        // Brightness is part of the state line while the light is on.
        if (before.light != after.light) add(ControlChange.STATE)
    }
}

private typealias ControlHolder = BaseViewHolder<ControlItem, ViewBinding, ControlAdapterCallback>

internal class ControlAdapter(
    callback: ControlAdapterCallback,
) : BaseListAdapter<ControlItem, ViewBinding, ControlAdapterCallback>(callback, ControlDiff) {

    override fun viewTypeOf(item: ControlItem): Int = when (item) {
        is ControlItem.Header -> TYPE_HEADER
        is ControlItem.Profile -> TYPE_PROFILE
        is ControlItem.Scenario -> TYPE_SCENARIO
        is ControlItem.Device -> TYPE_DEVICE
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ControlHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemControlHeaderBinding.inflate(inflater, parent, false), callback)
            TYPE_PROFILE -> ProfileHolder(ItemControlProfileBinding.inflate(inflater, parent, false), callback)
            TYPE_SCENARIO -> ScenarioHolder(ItemControlScenarioBinding.inflate(inflater, parent, false), callback)
            else -> DeviceHolder(ItemControlDeviceBinding.inflate(inflater, parent, false), callback)
        }

    private class HeaderHolder(
        private val views: ItemControlHeaderBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        override fun bind(item: ControlItem) {
            val header = item as ControlItem.Header
            bindTitle(header)
            bindSummary(header)
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) {
            val header = item as ControlItem.Header
            val changes = controlChanges(payloads) ?: return bind(item)
            if (ControlChange.TITLE in changes) bindTitle(header)
            if (ControlChange.SUMMARY in changes) bindSummary(header)
        }

        private fun bindTitle(header: ControlItem.Header) {
            views.controlHeaderTitle.text = header.title
        }

        private fun bindSummary(header: ControlItem.Header) {
            views.controlHeaderSummary.text = header.summary.orEmpty()
            views.controlHeaderSummary.setVisible(header.summary != null)
        }
    }

    /**
     * The profile is a mode, not a device: two segmented buttons instead of a
     * switch. The card itself is not clickable — only an explicit side acts,
     * because "toggle" is not a safe action for an at-least-once delivery.
     */
    private class ProfileHolder(
        private val views: ItemControlProfileBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        /** Programmatic `check` must not be reported as a user choice. */
        private var binding_ = false

        init {
            views.controlProfilePersonal.setText(R.string.control_profile_personal)
            views.controlProfileWork.setText(R.string.control_profile_work)
            views.controlProfileGroup.addOnButtonCheckedListener { _, checkedId, isChecked ->
                if (binding_ || !isChecked) return@addOnButtonCheckedListener
                val row = item as ControlItem.Profile
                callback.onProfileChanged(row.entry, checkedId == views.controlProfileWork.id)
            }
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Profile
            bindTitle(row.entry)
            bindIcon(row.entry)
            bindPower(row.entry)
            bindState(row.entry)
            bindBusy(row)
            bindAvailability(row.entry)
            bindEnabled(row)
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) {
            val row = item as ControlItem.Profile
            val changes = controlChanges(payloads) ?: return bind(item)
            if (ControlChange.TITLE in changes) bindTitle(row.entry)
            if (ControlChange.ICON in changes) bindIcon(row.entry)
            if (ControlChange.POWER in changes) bindPower(row.entry)
            if (
                ControlChange.STATE in changes ||
                ControlChange.POWER in changes ||
                ControlChange.AVAILABILITY in changes
            ) {
                bindState(row.entry)
            }
            if (ControlChange.BUSY in changes) bindBusy(row)
            if (ControlChange.AVAILABILITY in changes) bindAvailability(row.entry)
            if (ControlChange.BUSY in changes || ControlChange.AVAILABILITY in changes) bindEnabled(row)
        }

        private fun bindTitle(entry: ControlEntry) {
            views.controlProfileTitle.text = entry.title
        }

        private fun bindIcon(entry: ControlEntry) {
            views.controlProfileIcon.setImageResource(ControlIcons.resolve(entry))
        }

        private fun bindPower(entry: ControlEntry) {
            val work = entry.power == PowerState.ON
            val tint = MaterialColors.getColor(
                views.root,
                if (work) com.google.android.material.R.attr.colorOnPrimaryContainer
                else com.google.android.material.R.attr.colorOnSurfaceVariant,
            )
            views.controlProfileIcon.setColorFilter(tint)
            views.controlProfileIcon.isActivated = work
            // Neither side is claimed while the server state is unknown.
            val target = when (entry.power) {
                PowerState.ON -> views.controlProfileWork.id
                PowerState.OFF -> views.controlProfilePersonal.id
                null -> View.NO_ID
            }
            if (views.controlProfileGroup.checkedButtonId == target) return
            binding_ = true
            if (target == View.NO_ID) views.controlProfileGroup.clearChecked() else views.controlProfileGroup.check(target)
            binding_ = false
        }

        private fun bindState(entry: ControlEntry) {
            views.controlProfileState.setText(
                when {
                    !entry.available -> R.string.control_state_unavailable
                    entry.power == null -> R.string.control_profile_states_unknown
                    entry.power == PowerState.ON -> R.string.control_profile_work
                    else -> R.string.control_profile_personal
                },
            )
        }

        private fun bindBusy(row: ControlItem.Profile) {
            views.controlProfileProgress.setVisible(row.busy)
        }

        private fun bindAvailability(entry: ControlEntry) {
            views.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
        }

        private fun bindEnabled(row: ControlItem.Profile) {
            val enabled = row.entry.available && !row.busy
            views.controlProfileGroup.isEnabled = enabled
            views.controlProfilePersonal.isEnabled = enabled
            views.controlProfileWork.isEnabled = enabled
        }
    }

    private class ScenarioHolder(
        private val views: ItemControlScenarioBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        init {
            forwardRowClicks()
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Scenario
            bindTitle(row.entry)
            bindIcon(row.entry)
            bindBusy(row)
            bindAvailability(row.entry)
            bindEnabled(row)
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) {
            val row = item as ControlItem.Scenario
            val changes = controlChanges(payloads) ?: return bind(item)
            if (ControlChange.TITLE in changes) bindTitle(row.entry)
            if (ControlChange.ICON in changes) bindIcon(row.entry)
            if (ControlChange.BUSY in changes) bindBusy(row)
            if (ControlChange.AVAILABILITY in changes) bindAvailability(row.entry)
            if (ControlChange.BUSY in changes || ControlChange.AVAILABILITY in changes) bindEnabled(row)
        }

        private fun bindTitle(entry: ControlEntry) {
            views.controlScenarioTitle.text = entry.title
        }

        private fun bindIcon(entry: ControlEntry) {
            views.controlScenarioIcon.setImageResource(ControlIcons.resolve(entry))
        }

        private fun bindBusy(row: ControlItem.Scenario) {
            views.controlScenarioProgress.setVisible(row.busy)
        }

        private fun bindAvailability(entry: ControlEntry) {
            views.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
        }

        private fun bindEnabled(row: ControlItem.Scenario) {
            views.root.isEnabled = row.entry.available && !row.busy
        }
    }

    private class DeviceHolder(
        private val views: ItemControlDeviceBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        /** Programmatic `isChecked` must not be reported as a user toggle. */
        private var binding_ = false

        init {
            forwardRowClicks()
            views.controlDeviceSwitch.setOnCheckedChangeListener { _, checked ->
                if (!binding_) {
                    val row = item as ControlItem.Device
                    callback.onPowerChanged(row.entry, checked)
                }
            }
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Device
            bindTitle(row.entry)
            bindIcon(row.entry)
            bindPower(row.entry)
            bindState(row)
            bindBusy(row)
            bindAvailability(row.entry)
            bindEnabled(row)
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) {
            val row = item as ControlItem.Device
            val changes = controlChanges(payloads) ?: return bind(item)
            if (ControlChange.TITLE in changes) bindTitle(row.entry)
            if (ControlChange.ICON in changes) bindIcon(row.entry)
            if (ControlChange.POWER in changes) bindPower(row.entry)
            if (
                ControlChange.STATE in changes ||
                ControlChange.POWER in changes ||
                ControlChange.AVAILABILITY in changes
            ) {
                bindState(row)
            }
            if (ControlChange.BUSY in changes) bindBusy(row)
            if (ControlChange.AVAILABILITY in changes) bindAvailability(row.entry)
            if (ControlChange.BUSY in changes || ControlChange.AVAILABILITY in changes) bindEnabled(row)
        }

        private fun bindTitle(entry: ControlEntry) {
            views.controlDeviceTitle.text = entry.title
        }

        private fun bindIcon(entry: ControlEntry) {
            views.controlDeviceIcon.setImageResource(ControlIcons.resolve(entry))
        }

        private fun bindPower(entry: ControlEntry) {
            val on = entry.power == PowerState.ON
            val tint = MaterialColors.getColor(
                views.root,
                if (on) com.google.android.material.R.attr.colorOnPrimaryContainer
                else com.google.android.material.R.attr.colorOnSurfaceVariant,
            )
            views.controlDeviceIcon.setColorFilter(tint)
            views.controlDeviceIcon.isActivated = on
            // Do not animate the thumb when the state is already the target one.
            if (views.controlDeviceSwitch.isChecked == on) return
            binding_ = true
            views.controlDeviceSwitch.isChecked = on
            binding_ = false
        }

        private fun bindState(row: ControlItem.Device) {
            val entry = row.entry
            val on = entry.power == PowerState.ON
            views.controlDeviceState.setText(
                when {
                    !entry.available -> R.string.control_state_unavailable
                    entry.power == PowerState.ON -> R.string.control_state_on
                    entry.power == PowerState.OFF -> R.string.control_state_off
                    else -> R.string.control_state_unknown
                },
            )
            val brightness = entry.light?.takeIf { on && entry.available && it.brightness && it.brightnessPct > 0 }?.brightnessPct
            if (brightness != null) {
                views.controlDeviceState.text = views.root.context.getString(R.string.control_state_on_brightness, brightness)
            }
        }

        private fun bindBusy(row: ControlItem.Device) {
            views.controlDeviceProgress.setVisible(row.busy)
        }

        private fun bindAvailability(entry: ControlEntry) {
            views.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
        }

        private fun bindEnabled(row: ControlItem.Device) {
            val enabled = row.entry.available && !row.busy
            views.controlDeviceSwitch.isEnabled = enabled
            views.root.isEnabled = enabled
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_SCENARIO = 1
        const val TYPE_DEVICE = 2
        const val TYPE_PROFILE = 3
        const val DISABLED_ALPHA = 0.5f
    }
}
