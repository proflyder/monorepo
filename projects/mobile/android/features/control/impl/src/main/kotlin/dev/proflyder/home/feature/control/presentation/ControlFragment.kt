package dev.proflyder.home.feature.control.presentation

import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlTexts
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.QuickTiles
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.SpeedDialItems
import dev.proflyder.home.feature.control.WakeFeedback
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import kotlinx.coroutines.delay

class ControlFragment : BaseFragment<FragmentControlBinding>(FragmentControlBinding.Companion::inflate) {
    internal val viewModel: ControlViewModel by activityViewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private lateinit var adapter: ControlAdapter
    private lateinit var states: ListStateViews
    private lateinit var speedDialAdapter: SpeedDialAdapter
    private lateinit var dialBack: OnBackPressedCallback
    private var speedDialOpen = false
    private var lightSheet: LightSheet? = null
    private var volumeSheet: VolumeSheet? = null
    private var speakDialog: SpeakDialog? = null

    /**
     * The live rows arrive after the first content frame. Their item stays in
     * the list from the start (with a stable key), so that arrival inserts no
     * row above the device sections; this flag additionally pulls the list to
     * the top when the user has not scrolled yet.
     */
    private var hubWasEmpty = true

    /** The single callback instance shared with the adapter. */
    private val adapterCallback = object : BaseAdapterCallback<ControlItem>(), ControlAdapterCallback {
        override fun onItemClick(item: ControlItem) {
            when (item) {
                // A native/HA scenario tile runs at once; a device-level action
                // (speak) needs its text first.
                is ControlItem.Scenario ->
                    if (item.entry.kind == ControlKind.ACTION) openSpeakDialog(item.entry)
                    else viewModel.run(item.entry)
                // An adjustable light opens its sheet; anything else flips on a row tap.
                // The switch itself always reports its own target state.
                is ControlItem.Device ->
                    when {
                        item.entry.isVolume && item.entry.available -> openVolumeSheet(item.entry)
                        item.entry.light != null && item.entry.available -> openLightSheet(item.entry)
                        else -> viewModel.setPower(item.entry, !item.entry.isOn())
                    }
                // A device group itself does nothing: its rows forward their taps.
                // The profile is a mode: only its segmented buttons act.
                is ControlItem.Header, is ControlItem.Voice, is ControlItem.DeviceGroup,
                is ControlItem.Hub, is ControlItem.QuickActions,
                is ControlItem.Filters,
                -> Unit
            }
        }

        override fun onItemLongClick(item: ControlItem) {
            if (item is ControlItem.Device) showPinDialog(item.entry)
        }

        override fun onPowerChanged(entry: ControlEntry, on: Boolean) = viewModel.setPower(entry, on)

        override fun onButtonPressed(entry: ControlEntry) = viewModel.press(entry)

        override fun onProfileChanged(entry: ControlEntry, work: Boolean) = viewModel.setProfile(entry, work)

        override fun onToggleSettings(deviceId: String) = viewModel.toggleDeviceSettings(deviceId)

        override fun onHeaderClick(header: ControlItem.Header) {
            if (header.collapsible) viewModel.toggleRoom(header.section)
        }

        override fun onPinToggle(entry: ControlEntry) = showPinDialog(entry)

        override fun onHubCardClick(card: HubCard) = openHubSection(card)

        override fun onHubPrimary(card: HubCard) {
            when {
                card is HubCard.Alarm && hubAlarmNeedsChoice(card) -> showHubAlarmDisableChoice(card)
                else -> viewModel.hubPrimary(card)
            }
        }

        override fun onQuickTileClick(tile: QuickTile) {
            when (tile) {
                is QuickTile.Device ->
                    when {
                        tile.entry.isVolume && tile.entry.available -> openVolumeSheet(tile.entry)
                        tile.entry.light != null && tile.entry.available -> openLightSheet(tile.entry)
                        else -> viewModel.setPower(tile.entry, !tile.entry.isOn())
                    }
                is QuickTile.Speak -> openSpeakDialog(tile.entry)
                is QuickTile.ScenarioTile -> viewModel.runScenarioNow(tile.scenario)
                QuickTile.Call -> viewModel.callStation()
            }
        }

        override fun onQuickTileLongClick(tile: QuickTile) {
            when (tile) {
                // A light's long press is reserved for its adjustment sheet.
                is QuickTile.Device ->
                    when {
                        tile.entry.isVolume -> openVolumeSheet(tile.entry)
                        tile.entry.light != null -> openLightSheet(tile.entry)
                        else -> viewModel.toggleFavoriteRef(
                            FavoriteRef(FavoriteKind.DEVICE, tile.entry.id),
                            tile.entry.localizedDisplayName(requireContext()),
                        )
                    }
                is QuickTile.Speak ->
                    viewModel.toggleFavoriteRef(
                        FavoriteRef(FavoriteKind.ACTION, tile.entry.id),
                        tile.entry.localizedDisplayName(requireContext()),
                    )
                is QuickTile.ScenarioTile ->
                    viewModel.toggleFavoriteRef(
                        FavoriteRef(FavoriteKind.SCENARIO, tile.scenario.id),
                        tile.scenario.label,
                    )
                // The call tile is built in and cannot be unpinned.
                QuickTile.Call -> Unit
            }
        }

        override fun onFilterSelected(filter: ControlFilter) = viewModel.setFilter(filter)

        override fun onWakeAction(action: WakeFeedbackAction) = viewModel.markWake(action)
    }

    override fun onBindingCreated(binding: FragmentControlBinding, savedInstanceState: Bundle?) {
        adapter = ControlAdapter(adapterCallback)
        val spanCount = resources.getInteger(R.integer.control_columns)
        val deviceSpan = spanCount / resources.getInteger(R.integer.control_device_columns)
        binding.controlList.layoutManager = GridLayoutManager(requireContext(), spanCount).apply {
            spanSizeLookup = object : GridLayoutManager.SpanSizeLookup() {
                override fun getSpanSize(position: Int): Int = when (adapter.currentList.getOrNull(position)) {
                    is ControlItem.Scenario -> 1
                    is ControlItem.Device, is ControlItem.DeviceGroup -> deviceSpan
                    else -> spanCount
                }
            }
        }
        binding.controlList.adapter = adapter
        binding.controlList.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                if (speedDialOpen || dy == 0) return
                when {
                    dy > 0 -> binding.controlQuickFab.hide()
                    else -> binding.controlQuickFab.show()
                }
            }
        })

        speedDialAdapter = SpeedDialAdapter(
            onClick = { item -> runSpeedDialItem(item) },
            onLongClick = { item -> pinSpeedDialItem(item) },
        )
        binding.controlQuickList.layoutManager = LinearLayoutManager(requireContext())
        binding.controlQuickList.adapter = speedDialAdapter
        binding.controlQuickFab.setOnClickListener { toggleSpeedDial() }
        binding.controlQuickScrim.setOnClickListener { closeSpeedDial() }
        dialBack = object : OnBackPressedCallback(false) {
            override fun handleOnBackPressed() = closeSpeedDial()
        }
        requireActivity().onBackPressedDispatcher.addCallback(viewLifecycleOwner, dialBack)

        states = ListStateViews(
            progress = binding.controlProgress,
            empty = binding.controlEmpty,
            error = binding.controlError,
            content = binding.controlContent,
            errorText = binding.controlErrorText,
        )
        binding.controlRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.controlRetry.setOnClickListener { viewModel.refresh() }
        binding.controlOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }

        viewModel.state.observe { render(it) }

        repeatOnView {
            while (true) {
                delay(1_000)
                viewModel.tick()
            }
        }
    }

    internal fun openLightSheet(entry: ControlEntry) {
        lightSheet?.dismiss()
        lightSheet = LightSheet(requireContext(), entry) { adjust -> viewModel.adjust(entry, adjust) }
            .also { it.show() }
    }

    /**
     * Opens the station volume sheet. Either volume row opens the same sheet:
     * the media player row is the main slider, the announcement row the second.
     */
    internal fun openVolumeSheet(entry: ControlEntry) {
        volumeSheet?.dismiss()
        val volumes = viewModel.state.value.entries.filter { it.isVolume && it.deviceId == entry.deviceId }
        val main = volumes.firstOrNull { ControlAction.MUTE in it.actions } ?: entry
        val announce = volumes.firstOrNull { it.id != main.id }
        volumeSheet = VolumeSheet(
            context = requireContext(),
            entry = main,
            announceEntry = announce,
            onLevel = { target, level -> viewModel.setVolume(target, level) },
            onMuted = { target, muted -> viewModel.setVolumeMuted(target, muted) },
        ).also { it.show() }
    }

    internal fun openSpeakDialog(entry: ControlEntry) {
        speakDialog?.dismiss()
        speakDialog = SpeakDialog(requireContext()) { text -> viewModel.speak(entry, text) }
            .also { it.show() }
    }

    /**
     * The speed dial replaces the old quick-actions block. Its height is
     * capped so a long favourites list scrolls inside the card.
     */
    private fun updateSpeedDial(items: List<SpeedDialItem>) {
        speedDialAdapter.submitList(items)
        val row = resources.getDimensionPixelSize(R.dimen.control_speed_dial_item_height)
        val padding = resources.getDimensionPixelSize(R.dimen.control_speed_dial_padding) * 2
        val max = resources.getDimensionPixelSize(R.dimen.control_speed_dial_max_height)
        val target = (items.size * row + padding).coerceAtMost(max)
        val params = binding.controlQuickList.layoutParams
        if (params.height != target) {
            params.height = target
            binding.controlQuickList.layoutParams = params
        }
    }

    private fun toggleSpeedDial() {
        when (speedDialOpen) {
            true -> closeSpeedDial()
            false -> openSpeedDial()
        }
    }

    private fun openSpeedDial() {
        speedDialOpen = true
        binding.controlQuickScrim.setVisible(true)
        binding.controlQuickDial.setVisible(true)
        binding.controlQuickFab.show()
        syncSpeedDialFab()
        dialBack.isEnabled = true
    }

    internal fun closeSpeedDial() {
        if (!speedDialOpen) return
        speedDialOpen = false
        binding.controlQuickScrim.setVisible(false)
        binding.controlQuickDial.setVisible(false)
        binding.controlQuickFab.show()
        syncSpeedDialFab()
        dialBack.isEnabled = false
    }

    private fun syncSpeedDialFab() {
        binding.controlQuickFab.setImageResource(
            if (speedDialOpen) R.drawable.control_ic_close else R.drawable.control_ic_bolt,
        )
        binding.controlQuickFab.contentDescription = getString(
            if (speedDialOpen) R.string.control_quick_fab_close else R.string.control_quick_fab,
        )
    }

    /** A dial device flips on/off; the other items run their existing actions. */
    private fun runSpeedDialItem(item: SpeedDialItem) {
        closeSpeedDial()
        when (item) {
            is SpeedDialItem.Tile -> runQuickTileFromDial(item.tile)
            is SpeedDialItem.Wake -> viewModel.markWake(item.action)
        }
    }

    private fun runQuickTileFromDial(tile: QuickTile) {
        when (tile) {
            is QuickTile.Device -> viewModel.setPower(tile.entry, !tile.entry.isOn())
            is QuickTile.Speak -> openSpeakDialog(tile.entry)
            is QuickTile.ScenarioTile -> viewModel.runScenarioNow(tile.scenario)
            QuickTile.Call -> viewModel.callStation()
        }
    }

    /** Long press keeps the old quick-tile pin/unpin path for all favourites. */
    private fun pinSpeedDialItem(item: SpeedDialItem) {
        when (item) {
            is SpeedDialItem.Tile -> pinQuickTile(item.tile)
            is SpeedDialItem.Wake -> Unit
        }
    }

    private fun pinQuickTile(tile: QuickTile) {
        when (tile) {
            is QuickTile.Device -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.DEVICE, tile.entry.id),
                tile.entry.localizedDisplayName(requireContext()),
            )
            is QuickTile.Speak -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.ACTION, tile.entry.id),
                tile.entry.localizedDisplayName(requireContext()),
            )
            is QuickTile.ScenarioTile -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.SCENARIO, tile.scenario.id),
                tile.scenario.label,
            )
            QuickTile.Call -> Unit
        }
    }

    override fun onBindingDestroyed() {
        lightSheet?.dismiss()
        lightSheet = null
        volumeSheet?.dismiss()
        volumeSheet = null
        speakDialog?.dismiss()
        speakDialog = null
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh(silent = true)
    }

    private fun render(state: ControlUiState) {
        val hasItems = state.entries.isNotEmpty()
        val loadError = state.loadError
        when {
            !state.configured -> {
                states.showError(getString(R.string.control_not_configured))
                binding.controlRetry.setVisible(false)
                binding.controlOpenConnection.setVisible(true)
            }
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> {
                states.showError(loadError.userMessage(requireContext()))
                binding.controlRetry.setVisible(true)
                binding.controlOpenConnection.setVisible(false)
            }
            !hasItems -> states.showEmpty()
            else -> {
                states.showContent()
                val tiles = QuickTiles.build(
                    refs = state.favorites,
                    entries = state.entries,
                    scenarios = state.scenarios,
                    pendingEntries = state.pending,
                    pendingScenarios = state.pendingScenarios,
                )
                val wake = WakeFeedback.actions(state.entries, state.pending)
                updateSpeedDial(SpeedDialItems.build(tiles, wake))
                val leading = listOf(
                    ControlItem.Hub(state.hubCards, state.hubRemaining, state.nowUnix),
                )
                val stickToTop = hubWasEmpty &&
                    state.hubCards.isNotEmpty() &&
                    !binding.controlList.canScrollVertically(-1)
                hubWasEmpty = state.hubCards.isEmpty()
                val items = ControlItems.build(
                    entries = state.entries,
                    pending = state.pending,
                    statesOk = state.statesOk,
                    filter = state.filter,
                    texts = texts(),
                    expandedDevices = state.expandedDevices,
                    leading = leading,
                    collapsedRooms = state.collapsedRooms,
                    voiceCard = false,
                )
                adapter.submitList(items) { if (stickToTop) binding.controlList.scrollToPosition(0) }
            }
        }
        binding.controlRefresh.isRefreshing = state.refreshing && hasItems
        binding.controlStatesBanner.setVisible(hasItems && !state.statesOk)

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            viewModel.consumeActionError()
        }
        state.notice?.let { notice ->
            showNotice(notice)
            viewModel.consumeNotice()
        }
    }

    private fun showNotice(notice: ControlNotice) {
        when (notice) {
            is ControlNotice.ScenarioQueued ->
                Notices.show(binding.root, getString(R.string.control_scenario_queued, notice.title))
            is ControlNotice.FavoriteChanged ->
                Notices.show(
                    binding.root,
                    getString(
                        if (notice.pinned) R.string.control_favorite_pinned else R.string.control_favorite_unpinned,
                        notice.title,
                    ),
                )
            is ControlNotice.WakeMarked ->
                Notices.show(
                    binding.root,
                    getString(
                        when (notice.kind) {
                            WakeFeedbackKind.FALSE_ALARM -> R.string.control_wake_false_alarm_sent
                            WakeFeedbackKind.MISSED -> R.string.control_wake_missed_sent
                        },
                    ),
                )
            ControlNotice.CommandSent ->
                Notices.show(binding.root, getString(R.string.control_command_sent))
        }
    }

    /** Long press in a room group: pin the device to Quick actions or unpin it. */
    internal fun showPinDialog(entry: ControlEntry) {
        val ref = FavoriteRef(FavoriteKind.DEVICE, entry.id)
        val pinned = ref in viewModel.state.value.favorites
        confirm(
            message = entry.localizedDisplayName(requireContext()),
            positive = if (pinned) R.string.control_unpin else R.string.control_pin,
            negative = android.R.string.cancel,
            destructive = false,
        ) { viewModel.toggleFavorite(entry) }
    }

    /** Tapping a live row jumps to the section that owns it. */
    internal fun openHubSection(card: HubCard) {
        val featureId = when (card) {
            is HubCard.Alarm -> "alarms"
            is HubCard.TimerCard -> "timers"
            is HubCard.Reminder -> "reminders"
        }
        requireNavigator().selectTab(featureId)
    }

    /** True when the hub's "turn off" must ask skip-once vs disable-completely. */
    internal fun hubAlarmNeedsChoice(card: HubCard.Alarm): Boolean =
        !card.ringing &&
            card.schedule.enabled &&
            alarmDisableChoice(card.schedule) == AlarmDisableChoice.ASK

    internal fun showHubAlarmDisableChoice(card: HubCard.Alarm) {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.control_alarm_disable_title)
            .setMessage(R.string.control_alarm_disable_message)
            .setNeutralButton(R.string.control_alarm_disable_skip_once) { _, _ -> viewModel.skipAlarmOccurrence(card) }
            .setPositiveButton(R.string.control_alarm_disable_turn_off) { _, _ -> viewModel.disableAlarm(card) }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun texts() = ControlTexts(
        otherDevices = getString(R.string.control_section_other),
        scenarios = getString(R.string.control_section_scenarios),
        unavailable = getString(R.string.control_section_unavailable),
        voice = getString(R.string.control_section_voice),
        onSummary = { on, total -> getString(R.string.control_on_summary, on, total) },
        actions = getString(R.string.control_section_actions),
        all = getString(R.string.control_filter_all),
        lights = getString(R.string.control_filter_lights),
        switches = getString(R.string.control_filter_switches),
    )

    internal fun ControlEntry.isOn(): Boolean = power == dev.proflyder.home.core.model.PowerState.ON
}
