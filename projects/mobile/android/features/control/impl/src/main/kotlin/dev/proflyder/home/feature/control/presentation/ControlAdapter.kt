package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import androidx.recyclerview.widget.DiffUtil
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceGroupBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceRowBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFilterBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFiltersBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHeaderBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubRowBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlProfileBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlQuickActionsBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlQuickTileBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

/** Row taps come from [AdapterCallback]; the switch reports an explicit target state. */
internal interface ControlAdapterCallback : AdapterCallback<ControlItem> {
    fun onPowerChanged(entry: ControlEntry, on: Boolean)

    /** The profile selector reports the chosen side: `work = true` is the named profile. */
    fun onProfileChanged(entry: ControlEntry, work: Boolean)

    /** Opens or closes the settings rows of a device card. */
    fun onToggleSettings(deviceId: String)

    /** Folds or unfolds a room section (header tap). */
    fun onHeaderClick(header: ControlItem.Header)

    /** Pins or unpins a device from a long press in its room group. */
    fun onPinToggle(entry: ControlEntry)

    fun onHubCardClick(card: HubCard)

    fun onHubPrimary(card: HubCard)

    fun onQuickTileClick(tile: QuickTile)

    fun onQuickTileLongClick(tile: QuickTile)

    /** A device filter chip was chosen. */
    fun onFilterSelected(filter: ControlFilter)
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

    /** Any visible change inside a device group; the holder rebuilds its dynamic rows. */
    GROUP,

    /** The live-rows content changed; the holder rebuilds its rows in place. */
    HUB,
}

/** Identity of the artwork an entry resolves to; see [ControlIcons]. */
internal fun ControlEntry.iconIdentity(): Pair<Any, String> = kind to icon.lowercase()

/** Typed payloads delivered for one bind, merged into a single aspect set. */
internal fun controlChanges(payloads: List<Any>): PayloadSet<ControlChange>? =
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
            oldItem is ControlItem.DeviceGroup && newItem is ControlItem.DeviceGroup ->
                if (oldItem == newItem) emptySet() else setOf(ControlChange.GROUP)
            oldItem is ControlItem.Hub && newItem is ControlItem.Hub ->
                if (oldItem == newItem) emptySet() else setOf(ControlChange.HUB)
            oldItem is ControlItem.QuickActions && newItem is ControlItem.QuickActions -> emptySet()
            oldItem is ControlItem.Filters && newItem is ControlItem.Filters -> emptySet()
            else -> emptySet()
        }
        return if (changed.isEmpty()) null else PayloadSet.of(changed)
    }

    private fun headerChanges(old: ControlItem.Header, new: ControlItem.Header): Set<ControlChange> = buildSet {
        if (old.title != new.title) add(ControlChange.TITLE)
        if (old.summary != new.summary) add(ControlChange.SUMMARY)
        if (old.collapsed != new.collapsed) add(ControlChange.SUMMARY)
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

/** Artwork of one live row, shared with no new image per entity. */
internal fun hubIcon(card: HubCard): Int = when (card) {
    is HubCard.Alarm -> R.drawable.control_ic_alarm
    is HubCard.TimerCard -> R.drawable.control_ic_timer
    is HubCard.Reminder -> R.drawable.control_ic_bell
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
        is ControlItem.DeviceGroup -> TYPE_GROUP
        is ControlItem.Hub -> TYPE_HUB
        is ControlItem.QuickActions -> TYPE_QUICK
        is ControlItem.Filters -> TYPE_FILTERS
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ControlHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemControlHeaderBinding.inflate(inflater, parent, false), callback)
            TYPE_PROFILE -> ProfileHolder(ItemControlProfileBinding.inflate(inflater, parent, false), callback)
            TYPE_SCENARIO -> ScenarioHolder(ItemControlScenarioBinding.inflate(inflater, parent, false), callback)
            TYPE_GROUP -> DeviceGroupHolder(ItemControlDeviceGroupBinding.inflate(inflater, parent, false), callback)
            TYPE_HUB -> HubHolder(ItemControlHubBinding.inflate(inflater, parent, false), callback)
            TYPE_QUICK -> QuickActionsHolder(ItemControlQuickActionsBinding.inflate(inflater, parent, false), callback)
            TYPE_FILTERS -> FiltersHolder(ItemControlFiltersBinding.inflate(inflater, parent, false), callback)
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
            bindCollapsed(header)
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) {
            val header = item as ControlItem.Header
            val changes = controlChanges(payloads) ?: return bind(item)
            if (ControlChange.TITLE in changes) bindTitle(header)
            if (ControlChange.SUMMARY in changes) bindSummary(header)
            bindCollapsed(header)
        }

        private fun bindTitle(header: ControlItem.Header) {
            views.controlHeaderTitle.text = header.title
        }

        private fun bindSummary(header: ControlItem.Header) {
            views.controlHeaderSummary.text = header.summary.orEmpty()
            views.controlHeaderSummary.setVisible(header.summary != null)
        }

        private fun bindCollapsed(header: ControlItem.Header) {
            views.controlHeaderChevron.setVisible(header.collapsible)
            // Expanded points up, collapsed points down.
            views.controlHeaderChevron.rotation = if (header.collapsed) 0f else 180f
            views.root.setOnClickListener {
                if (header.collapsible) callback.onHeaderClick(header)
            }
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

    /**
     * The compact "next up" block: one line per live item, at most
     * [dev.proflyder.home.feature.control.HubCards.MAX_ROWS] rows. There is no
     * pager and no big button: the trailing control is a switch for alarms and
     * a small pause/resume button for timers.
     */
    private class HubHolder(
        private val views: ItemControlHubBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        private val texts = HubCardTexts(views.root.context)

        override fun bind(item: ControlItem) {
            val hub = item as ControlItem.Hub
            views.controlHubRows.removeAllViews()
            hub.cards.forEach { card -> addRow(hub, card) }
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) = bind(item)

        internal fun addRow(hub: ControlItem.Hub, card: HubCard) {
            val row = ItemControlHubRowBinding.inflate(
                LayoutInflater.from(views.root.context),
                views.controlHubRows,
                false,
            )
            row.hubRowIcon.setImageResource(hubIcon(card))
            row.hubRowTime.text = texts.timeText(card, hub.remaining[card.key], hub.nowUnix)
            row.hubRowLabel.text = texts.label(card)
            bindTrailing(row, card)
            row.root.setOnClickListener { callback.onHubCardClick(card) }
            views.controlHubRows.addView(row.root)
        }

        internal fun bindTrailing(row: ItemControlHubRowBinding, card: HubCard) {
            when (card) {
                is HubCard.Alarm -> bindAlarmTrailing(row, card)
                is HubCard.TimerCard -> bindTimerTrailing(row, card)
                is HubCard.Reminder -> hideTrailing(row)
            }
        }

        internal fun bindAlarmTrailing(row: ItemControlHubRowBinding, card: HubCard.Alarm) {
            row.hubRowAction.setVisible(false)
            row.hubRowSwitch.setVisible(true)
            row.hubRowSwitch.contentDescription = texts.title(card)
            row.hubRowSwitch.isChecked = card.schedule.enabled
            row.hubRowSwitch.setOnCheckedChangeListener { _, checked ->
                if (!checked) toggleAlarmOff(row, card)
            }
        }

        /**
         * A switch flip is optimistic; put it back until the server confirms.
         * The recurring-alarm skip/disable dialog then decides what actually
         * happens, so a cancelled dialog never leaves a false off state.
         */
        internal fun toggleAlarmOff(row: ItemControlHubRowBinding, card: HubCard.Alarm) {
            row.hubRowSwitch.isChecked = true
            callback.onHubPrimary(card)
        }

        internal fun bindTimerTrailing(row: ItemControlHubRowBinding, card: HubCard.TimerCard) {
            row.hubRowSwitch.setVisible(false)
            row.hubRowAction.setVisible(true)
            val paused = card.timer.state == TimerState.PAUSED
            row.hubRowAction.setImageResource(
                if (paused) R.drawable.control_ic_play else R.drawable.control_ic_pause,
            )
            row.hubRowAction.contentDescription = views.root.context.getString(
                if (paused) R.string.control_hub_resume else R.string.control_hub_pause,
            )
            row.hubRowAction.setOnClickListener { callback.onHubPrimary(card) }
        }

        internal fun hideTrailing(row: ItemControlHubRowBinding) {
            row.hubRowSwitch.setVisible(false)
            row.hubRowAction.setVisible(false)
        }
    }

    private class QuickActionsHolder(
        private val views: ItemControlQuickActionsBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        override fun bind(item: ControlItem) {
            val quick = item as ControlItem.QuickActions
            views.controlQuickTiles.removeAllViews()
            views.controlQuickHint.setVisible(quick.hint)
            quick.tiles.chunked(COLUMNS).forEach { rowTiles ->
                views.controlQuickTiles.addView(quickRow(rowTiles))
            }
        }

        private fun quickRow(rowTiles: List<QuickTile>): LinearLayout {
            val row = LinearLayout(views.root.context).apply {
                orientation = LinearLayout.HORIZONTAL
            }
            rowTiles.forEach { tile -> row.addView(tileView(row, tile)) }
            // Keep the last row aligned with the grid instead of stretching.
            repeat(COLUMNS - rowTiles.size) {
                row.addView(
                    View(views.root.context),
                    LinearLayout.LayoutParams(0, 1, 1f),
                )
            }
            return row
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) = bind(item)

        private fun tileView(parent: LinearLayout, tile: QuickTile): View {
            val views = ItemControlQuickTileBinding.inflate(
                LayoutInflater.from(parent.context),
                parent,
                false,
            )
            views.quickTileLabel.text = when (tile) {
                is QuickTile.Device -> tile.entry.displayName
                is QuickTile.ScenarioTile -> tile.scenario.label
                is QuickTile.Speak -> tile.entry.displayName
                QuickTile.Call -> views.root.context.getString(R.string.control_call_title)
            }
            views.quickTileIcon.setImageResource(
                when (tile) {
                    is QuickTile.Device -> when (tile.entry.kind) {
                        ControlKind.LIGHT -> R.drawable.control_ic_bulb
                        ControlKind.SWITCH -> R.drawable.control_ic_power
                        else -> ControlIcons.resolve(tile.entry)
                    }
                    is QuickTile.ScenarioTile -> R.drawable.control_ic_play_circle
                    is QuickTile.Speak -> R.drawable.control_ic_voice
                    QuickTile.Call -> R.drawable.control_ic_call
                },
            )
            val busy = when (tile) {
                is QuickTile.Device -> tile.busy
                is QuickTile.ScenarioTile -> tile.busy
                is QuickTile.Speak -> tile.busy
                QuickTile.Call -> false
            }
            views.root.alpha = if (busy) DISABLED_ALPHA else 1f
            views.root.setOnClickListener { callback.onQuickTileClick(tile) }
            views.root.setOnLongClickListener {
                callback.onQuickTileLongClick(tile)
                true
            }
            return views.root
        }
    }

    private class FiltersHolder(
        private val views: ItemControlFiltersBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        private var rendered: List<ControlItem.FilterChip> = emptyList()
        private var filterByViewId: Map<Int, ControlFilter> = emptyMap()
        private var syncing = false

        init {
            views.controlFilters.setOnCheckedStateChangeListener { _, checkedIds ->
                if (syncing) return@setOnCheckedStateChangeListener
                val id = checkedIds.firstOrNull() ?: return@setOnCheckedStateChangeListener
                filterByViewId[id]?.let { callback.onFilterSelected(it) }
            }
        }

        override fun bind(item: ControlItem) {
            val filters = item as ControlItem.Filters
            if (filters.chips != rendered) {
                rebuild(filters.chips)
            }
            syncing = true
            filterByViewId.entries.firstOrNull { it.value == filters.selected }?.let {
                views.controlFilters.check(it.key)
            }
            syncing = false
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) = bind(item)

        private fun rebuild(chips: List<ControlItem.FilterChip>) {
            rendered = chips
            views.controlFilters.removeAllViews()
            val byId = LinkedHashMap<Int, ControlFilter>()
            chips.forEach { chip -> addChip(chip, byId) }
            filterByViewId = byId
        }

        private fun addChip(chip: ControlItem.FilterChip, byId: MutableMap<Int, ControlFilter>) {
            val view = ItemControlFilterBinding.inflate(
                LayoutInflater.from(views.root.context),
                views.controlFilters,
                false,
            ).root
            view.id = View.generateViewId()
            view.text = chip.label
            views.controlFilters.addView(view)
            byId[view.id] = chip.filter
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

    /**
     * A device with several entries: one card whose rows are inflated from
     * [ItemControlDeviceRowBinding]. The rows are dynamic, so any payload simply
     * rebinds the card in place (the payload still keeps the holder, so no
     * crossfade) and the settings list is toggled by the view model's state.
     */
    private class DeviceGroupHolder(
        private val views: ItemControlDeviceGroupBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        override fun bind(item: ControlItem) {
            val group = item as ControlItem.DeviceGroup
            // A card with one primary has no header: that row itself is the
            // device title (see ControlItems.DeviceGroup).
            views.controlGroupTitle.text = group.title
            views.controlGroupTitle.setVisible(group.primary.size != 1)
            views.controlGroupPrimary.removeAllViews()
            group.primary.forEach { addRow(views.controlGroupPrimary, it, group.statesKnown) }
            views.controlGroupActions.removeAllViews()
            group.actions.forEach { addRow(views.controlGroupActions, it, group.statesKnown) }
            views.controlGroupActions.setVisible(group.actions.isNotEmpty())
            views.controlGroupSettings.text =
                views.root.context.getString(R.string.control_device_settings, group.settings.size)
            views.controlGroupSettings.setVisible(group.settings.isNotEmpty())
            val showSettings = group.expanded && group.settings.isNotEmpty()
            views.controlGroupSettingsList.removeAllViews()
            if (showSettings) {
                group.settings.forEach { addRow(views.controlGroupSettingsList, it, group.statesKnown) }
            }
            views.controlGroupSettingsList.setVisible(showSettings)
            views.controlGroupSettings.setOnClickListener { callback.onToggleSettings(group.deviceId) }
        }

        override fun bindPayloads(item: ControlItem, payloads: List<Any>) = bind(item)

        internal fun addRow(
            container: LinearLayout,
            row: ControlItem.DeviceGroup.Row,
            statesKnown: Boolean,
        ) {
            val rowViews = ItemControlDeviceRowBinding.inflate(
                LayoutInflater.from(container.context),
                container,
                false,
            )
            val entry = row.entry
            rowViews.controlRowTitle.text =
                entry.title.takeUnless { it.isBlank() || it == entry.id } ?: entry.device.ifBlank { entry.title }
            rowViews.controlRowIcon.setImageResource(ControlIcons.resolve(entry))
            val on = entry.power == PowerState.ON
            val tint = MaterialColors.getColor(
                rowViews.root,
                if (on) com.google.android.material.R.attr.colorOnPrimaryContainer
                else com.google.android.material.R.attr.colorOnSurfaceVariant,
            )
            rowViews.controlRowIcon.setColorFilter(tint)
            rowViews.controlRowIcon.isActivated = on
            // Set the programmatic state before installing the listener, so
            // binding a row can never look like a user toggle.
            rowViews.controlRowSwitch.isChecked = on
            rowViews.controlRowSwitch.setOnCheckedChangeListener { _, checked ->
                callback.onPowerChanged(entry, checked)
            }
            rowViews.controlRowState.setText(
                when {
                    !entry.available -> R.string.control_state_unavailable
                    entry.power == PowerState.ON -> R.string.control_state_on
                    entry.power == PowerState.OFF -> R.string.control_state_off
                    else -> R.string.control_state_unknown
                },
            )
            val brightness = entry.light?.takeIf { on && entry.available && it.brightness && it.brightnessPct > 0 }?.brightnessPct
            if (brightness != null) {
                rowViews.controlRowState.text =
                    rowViews.root.context.getString(R.string.control_state_on_brightness, brightness)
            }
            rowViews.controlRowProgress.setVisible(row.busy)
            rowViews.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
            val enabled = entry.available && !row.busy
            rowViews.controlRowSwitch.isEnabled = enabled
            rowViews.root.isEnabled = enabled
            rowViews.root.setOnClickListener {
                callback.onItemClick(ControlItem.Device(entry, row.busy, statesKnown))
            }
            rowViews.root.setOnLongClickListener {
                callback.onPinToggle(entry)
                true
            }
            container.addView(rowViews.root)
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_SCENARIO = 1
        const val TYPE_DEVICE = 2
        const val TYPE_PROFILE = 3
        const val TYPE_GROUP = 4
        const val TYPE_HUB = 5
        const val TYPE_QUICK = 6
        const val TYPE_FILTERS = 7
        const val DISABLED_ALPHA = 0.5f
        const val COLUMNS = 3
    }
}
